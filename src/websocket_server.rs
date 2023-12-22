
use websocket::{OwnedMessage, CloseData, WebSocketError, WebSocketResult};
use websocket::sync::Server;
use websocket::receiver::Reader;
use websocket::sender::Writer;

use std::thread::{Builder, sleep};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::{self, Display};
use std::net::TcpStream;
use std::io::ErrorKind as IoErrorKind;
use std::time::{Instant, Duration};

use rand::{Rng, thread_rng};
use collect_result::CollectResult;

use crate::card::{Card, Deck};
use crate::hand::Hand;

type WsReader = Reader<TcpStream>;
type WsWriter = Writer<TcpStream>;

struct Player(WsReader, WsWriter);

pub struct Session {
    players: Vec<Player>,
    pub game: Game,
}

impl Session {
    #[inline]
    fn new(host: (WsReader, WsWriter), game: Game) -> Self {
        Session {
            players: vec![Player(host.0, host.1)],
            game,
        }
    }
    fn ping(&mut self) -> WebSocketResult<()> {
        let ping_msg = OwnedMessage::Ping(vec![b'P', b'I', b'n', b'G']);

        for player in &mut self.players {
            player.1.send_message(&ping_msg)?;
        }

        Ok(())
    }
    fn handle(&mut self) -> WebSocketResult<()> {
        let mut cmds = Vec::new();

        // CHECK ONLY WHEN NEW GAME STARTS
        if self.game.deck.size() < 20 {
            self.game.deck = Deck::new_standard();
            self.game.deck.shuffle();
            self.game.dirty_deck = true;
        }

        if self.game.dealer_hand.cards().is_empty() {
            let hole_player = self.game.deck.draw_one().unwrap();
            let hole_dealer = self.game.deck.draw_one().unwrap();
            let open_player = self.game.deck.draw_one().unwrap();
            let open_dealer = self.game.deck.draw_one().unwrap();

            self.game.dealer_hand = Hand::new([hole_dealer, open_dealer]);
            self.game.player_hand = Hand::new([hole_player, open_player]);

            let pcards = self.game.player_hand.cards();
            cmds.push(Command::HoleCards(pcards[0], pcards[1..].to_vec()).into_message());
            cmds.push(Command::DealerHole(self.game.dealer_hand.cards()[0]).into_message());
        }

        if self.game.dirty_deck {
            self.game.dirty_deck = false;
            cmds.push(Command::DeckSize(self.game.deck.size() as u8).into_message());
        }

        for player in &mut self.players {
            for cmd in &cmds {
                player.1.send_message(cmd)?;
            }

            match handle(&mut player.0, &mut player.1)? {
                Command::Hit => {
                    if let Some(card) = self.game.deck.draw_one() {
                        self.game.dirty_deck = true;
                        player.1.send_message(&Command::Drawn(card).into_message())?;
                    }
                }
                Command::Stand => {
                    self.game.dealer_hand = Hand::new([]);
                    player.1.send_message(&Command::Lose.into_message())?;
                }
                _ => (),
            }
        }

        Ok(())
    }
    fn can_join(&self) -> bool {
        self.game.has_space()
    }
    #[must_use]
    fn join(&mut self, new_player: (WsReader, WsWriter)) -> WebSocketResult<()> {
        assert!(self.can_join());
        if self.game.has_space() {
            self.players.push(Player(new_player.0, new_player.1));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct  Game {
    deck: Deck,
    dealer_hand: Hand,
    player_hand: Hand,
    dirty_deck: bool,
}
impl Game {
    fn new() -> Game {
        Game {
            deck: Deck::empty(),
            dealer_hand: Hand::new([]),
            player_hand: Hand::new([]),
            dirty_deck: true,
        }
    }
    fn has_space(&self) -> bool {
        false
    }
}

fn gen_game_code() -> u16 {
    thread_rng().gen()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Host(Option<String>),
    Join(u16),
    HostOk(u16),
    JoinOk(u16, Option<String>),
    Start,
    Draw,
    Drawn(Card),
    DeckSize(u8),
    // Blackjack
    DealerHole(Card),
    HoleCards(Card, Vec<Card>),
    Stand,
    Hit,
    DoubleDown,
    Surrender,
    Split,
    // Misc
    ChatMsg(String, String),
    Chat(String),
    Win,
    Lose,
    Nop
}

impl Command {
    fn into_message(self) -> OwnedMessage {
        OwnedMessage::Text(self.to_string())
    } 
}

impl FromStr for Command {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split(' ');
        match split.next().ok_or(())? {
            "HOST" => Ok(Command::Host(split.next().map(|s| s.to_owned()))),
            "JOIN" => Ok(Command::Join(u16::from_str_radix(split.next().ok_or(())?, 16).map_err(|_| ())?)),
            "HOST_OK" => Ok(Command::HostOk(u16::from_str_radix(split.next().ok_or(())?, 16).map_err(|_| ())?)),
            "JOIN_OK" => Ok(Command::JoinOk(u16::from_str_radix(split.next().ok_or(())?, 16).map_err(|_| ())?, split.next().map(|s| s.to_owned()))),
            "START" => Ok(Command::Start),
            "DRAW" => Ok(Command::Draw),
            "DRAWN" => Ok(Command::Drawn(
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?)
            )),
            "DECKSIZE" => Ok(Command::DeckSize(
                split.next().ok_or(())?.parse().map_err(|_| ())?
            )),
            "DEALERHOLE" => Ok(Command::DealerHole(
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?)
            )),
            "HOLECARDS" => Ok(Command::HoleCards(
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?),
                split.map(|c| c.parse().map_err(|_| ()).map(Card::from_u8)).collect_result::<Vec<_>>()?,
            )),
            "STAND" => Ok(Command::Stand),
            "HIT" => Ok(Command::Hit),
            "DOUBLEDOWN" => Ok(Command::DoubleDown),
            "SURRENDER" => Ok(Command::Surrender),
            "SPLIT" => Ok(Command::Split),
            "CHAT_MSG" => Ok(Command::ChatMsg(
                split.next().ok_or(())?.to_owned(),
                split.collect::<Vec<&str>>().join(" ")
            )),
            "CHAT" => Ok(Command::Chat(
                split.collect::<Vec<&str>>().join(" ")
            )),
            "WIN" => Ok(Command::Win),
            "LOSE" => Ok(Command::Lose),
            "NOP" => Ok(Command::Nop),
            _ => Err(())
        }
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Host(Some(s)) => write!(f, "HOST {s}"),
            Command::Host(None) => write!(f, "HOST"),
            Command::Join(c) => write!(f, "JOIN {c:X}"),
            Command::HostOk(c) => write!(f, "HOST_OK {c:X}"),
            Command::JoinOk(c, Some(s2)) => write!(f, "JOIN_OK {c:X} {s2}"),
            Command::JoinOk(c, None) => write!(f, "JOIN_OK {c:X}"),
            Command::Start => write!(f, "START"),
            Command::Draw => write!(f, "DRAW"),
            Command::Drawn(c) => write!(f, "DRAWN {c}"),
            Command::DeckSize(n) => write!(f, "DECKSIZE {n}"),
            Command::DealerHole(c) => write!(f, "DEALERHOLE {c}"),
            Command::HoleCards(d, ps) => {
                write!(f, "HOLECARDS {d}")?;
                for p in ps {
                    write!(f, " {p}")?;
                }
                Ok(())
            }
            Command::Stand => write!(f, "STAND"),
            Command::Hit => write!(f, "HIT"),
            Command::DoubleDown => write!(f, "DOUBLEDOWN"),
            Command::Surrender => write!(f, "SURRENDER"),
            Command::Split => write!(f, "SPLIT"),
            Command::ChatMsg(p, m) => write!(f, "CHAT_MSG {p} {m}"),
            Command::Chat(m) => write!(f, "CHAT {m}"),
            Command::Win => write!(f, "WIN"),
            Command::Lose => write!(f, "LOSE"),
            Command::Nop => write!(f, ""),
        }
    }
}

fn handle(reader: &mut WsReader, sender: &mut WsWriter) -> WebSocketResult<Command> {
    let message = match reader.recv_message() {
        Err(WebSocketError::IoError(i)) if i.kind() == IoErrorKind::WouldBlock => return Ok(Command::Nop),
        m => m?,
    };

    match message {
        OwnedMessage::Close(_) => {
            let message = OwnedMessage::Close(None);
            sender.send_message(&message)?;
            return Err(WebSocketError::NoDataAvailable);
        }
        OwnedMessage::Pong(_) => (),
        OwnedMessage::Ping(ping) => {
            let message = OwnedMessage::Pong(ping);
            sender.send_message(&message)?;
        }
        OwnedMessage::Text(text) => {
            return text.parse().map_err(|()| WebSocketError::ProtocolError("indiscernable message"));
        }
        _ => eprintln!("Got unexpected {:?}", message),
    }
    Ok(Command::Nop)
}

#[derive(Clone)]
pub struct WebSocketServer {
    pub games: Arc<Mutex<HashMap<u16, Session>>>,
}

impl WebSocketServer {
    pub fn new() -> Self {
        WebSocketServer {
            games: Arc::new(Mutex::new(HashMap::<u16, Session>::new())),
        }
    }
    pub fn run(self) {
        let server = Server::bind("127.0.0.1:2794").unwrap();
        const PROTOCOL: &str = "fellestrekk";

        let WebSocketServer{games} = self;

        {
            let games = games.clone();
            let mut last_ping_time = Instant::now();

            Builder::new().name("running games handler".to_owned()).spawn(move || {
                loop {
                    {
                        let mut games_lock = games.lock().unwrap();

                        let mut deads = Vec::new();

                        const PING_DELAY: Duration = Duration::from_secs(5);

                        let now = Instant::now();

                        let ping = now - last_ping_time >= PING_DELAY;
                        if ping {
                            last_ping_time = now;
                        }

                        for (code, session) in games_lock.iter_mut() {
                            if ping {
                                let _ = session.ping();
                            }

                            match session.handle() {
                                Ok(()) => (),
                                Err(WebSocketError::ProtocolError(s)) => eprintln!("Protocol error: {}", s),
                                Err(WebSocketError::NoDataAvailable) => {
                                    deads.push(code.clone());
                                }
                                Err(WebSocketError::IoError(i)) if i.kind() == IoErrorKind::BrokenPipe => {
                                    deads.push(code.clone());
                                }
                                Err(e) => eprintln!("Unexpected error: {:?}", e),
                            }
                        }

                        deads.dedup();
                        for dead in deads {
                            games_lock.remove(&dead);
                        }
                    }
                    sleep(std::time::Duration::from_nanos(50));
                }
            }).unwrap();
        }

        for request in server.filter_map(Result::ok) {
            let games = games.clone();

            let ip = request.stream.peer_addr().unwrap();
            // Spawn a new thread for each connection.
            Builder::new().name(format!("connection_{}", ip)).spawn(move || {
                // Is this is not a fellestrekk connection, reject it
                if !request.protocols().contains(&PROTOCOL.to_owned()) {
                    request.reject().unwrap();
                    return;
                }
                // Accept using protocol
                let mut client = request.use_protocol(PROTOCOL).accept().unwrap();

                let code;

                let msg = client.recv_message().unwrap();

                match msg {
                    OwnedMessage::Text(s) => {
                        match s.parse() {
                            Ok(Command::Join(the_code)) => {
                                code = the_code;
                                if let Some(session) = games.lock().unwrap().get_mut(&code) {
                                    if session.can_join() {
                                        client.send_message(&Command::JoinOk(code, None).into_message()).unwrap();
                                        client.set_nonblocking(true).unwrap();
                                        session.join(client.split().unwrap()).unwrap();
                                    } else {
                                        client.send_message(&OwnedMessage::Close(Some(CloseData{
                                            status_code: 1008,
                                            reason: "Game is full".to_owned(),
                                        }))).unwrap();
                                    }
                                } else {
                                    client.send_message(&OwnedMessage::Close(Some(CloseData{
                                        status_code: 1008,
                                        reason: "No such game".to_owned(),
                                    }))).unwrap();
                                }
                            }
                            Ok(Command::Host(s)) => {
                                let _game = s.as_ref().map(|s| &**s);

                                code = loop {
                                    let code = gen_game_code();

                                    if !games.lock().unwrap().contains_key(&code) {
                                        break code;
                                    }
                                };
                                client.send_message(&Command::HostOk(code.clone()).into_message()).unwrap();
                                client.set_nonblocking(true).unwrap();
                                games.lock().unwrap().insert(code.clone(), Session::new(client.split().unwrap(), Game::new()));
                            }
                            Ok(c) => panic!("didn't except: {:?}", c),
                            Err(_) => panic!("Couldn't parse {:?}", s),
                        }
                    }
                    s => panic!("didn't expect {:?}", s)
                }
            }).unwrap();
        }
    }
}