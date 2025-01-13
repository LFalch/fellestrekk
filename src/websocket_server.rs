use websocket::{OwnedMessage, CloseData, WebSocketError, WebSocketResult};
use websocket::sync::Server;
use websocket::receiver::Reader;
use websocket::sender::Writer;

use std::cmp::Ordering::{Equal, Greater, Less};
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
use crate::hand::{BlackjackExt, Hand};
use crate::dealer::Dealer;

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

        self.game.tick(|cmd: Command| cmds.push(cmd.into_message()));

        for (i, player) in self.players.iter_mut().enumerate() {
            for cmd in &cmds {
                player.1.send_message(cmd)?;
            }

            match handle(&mut player.0, &mut player.1)? {
                Command::Bet(bet) => self.game.bet(i, bet, |cmd: Command| player.1.send_message(&cmd.into_message()).unwrap()),
                Command::Hit => self.game.hit(i, |cmd: Command| player.1.send_message(&cmd.into_message()).unwrap()),
                Command::Stand => self.game.stand(i, |cmd: Command| player.1.send_message(&cmd.into_message()).unwrap()),
                Command::DoubleDown => self.game.double_down(i, |cmd| player.1.send_message(&cmd.into_message()).unwrap()),
                Command::Surrender => self.game.surrender(i, |cmd| player.1.send_message(&cmd.into_message()).unwrap()),
                Command::Split => self.game.split(i, |cmd| player.1.send_message(&cmd.into_message()).unwrap()),
                Command::Start => {
                    if self.game.game_over {
                        self.game.dealer_hand = Hand::new([]);
                    }
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
    game_over: bool,
    dealer_turn: bool,
    dealer: Dealer,

    bet: u32,
}
// TODO: implement naturals (blackjacks)
impl Game {
    fn new() -> Game {
        Game {
            deck: Deck::empty(),
            dealer_hand: Hand::new([]),
            player_hand: Hand::new([]),
            dirty_deck: true,
            game_over: true,
            dealer_turn: false,
            dealer: Dealer::h17(),
            bet: 0,
        }
    }
    fn draw_card(&mut self) -> Card {
        self.dirty_deck = true;
        self.deck.draw_one().unwrap()
    }
    fn tick(&mut self, mut send: impl FnMut(Command)) {
        if self.game_over && self.bet == 0 {
            // wait for bet
            return;
        }

        if self.dealer_hand.cards().is_empty() {
            send(Command::Start);
            self.game_over = false;
            self.dealer_turn = false;
            if self.deck.size() < 20 {
                self.deck = Deck::new_standard();
                self.deck.shuffle();
            }
            let down_player = self.draw_card();
            let down_dealer = self.draw_card();
            let open_player = self.draw_card();
            let open_dealer = self.draw_card();

            self.dealer_hand = Hand::new([down_dealer, open_dealer]);
            self.player_hand = Hand::new([down_player, open_player]);

            let hand = Hand::new([open_dealer]);

            send(Command::ValueUpdate(None, hand.value(), hand.is_soft()));
            send(Command::DownCard(down_player));
            send(Command::PlayerDraw(0, open_player));
            send(Command::DealerDraw(self.dealer_hand.cards()[1]));

            let split = self.player_hand.cards()[0].suit_rank().1 == self.player_hand.cards()[1].suit_rank().1;

            send(Command::ValueUpdate(Some(0), self.player_hand.value(), self.player_hand.is_soft()));
            if self.player_hand.is_natural() {
                self.stand(0, &mut send);
            } else {
                send(Command::Status { hit: true, stand: true, double: true, surrender: true, split, new_game: false });
            }
        }

        if self.dealer_turn && !self.game_over {
            while self.dealer.hits(&self.dealer_hand) {
                let card = self.draw_card();
                self.dealer_hand.add_card(card);
                send(Command::DealerDraw(card));
            }
            self.game_over = true;
            send(Command::RevealDowns(self.dealer_hand.cards()[0], vec![self.player_hand.cards()[0]]));
            send(Command::ValueUpdate(None, self.dealer_hand.value(), self.dealer_hand.is_soft()));
            let bet = self.bet;
            self.bet = 0;
            match self.player_hand.cmp(&self.dealer_hand) {
                Less => send(Command::Lose),
                Greater => {
                    send(Command::Win);
                    if self.player_hand.is_natural() {
                        // blackjack bonus
                        send(Command::SendMoney(bet * 2 + bet / 2));
                    } else {
                        send(Command::SendMoney(bet * 2));
                    }
                }
                Equal => {
                    send(Command::Draw);
                    send(Command::SendMoney(bet));
                }
            }
        }

        if self.dirty_deck {
            self.dirty_deck = false;
            send(Command::DeckSize(self.deck.size() as u8));
        }
    }
    fn bet(&mut self, _pn: usize, bet: u32, mut send: impl FnMut(Command)) {
        if self.bet != 0 {
            return;
        }
        send(Command::TakeMoney(bet));
        self.bet = bet;
    }
    fn hit(&mut self, pn: usize, mut send: impl FnMut(Command)) {
        if self.dealer_turn || self.game_over || self.bet == 0 {
            return;
        }
        let card = self.draw_card();
        self.player_hand.add_card(card);
        send(Command::PlayerDraw(pn, card));

        let value = self.player_hand.value();
        send(Command::ValueUpdate(Some(0), value, self.player_hand.is_soft()));
        if self.player_hand.value() > 21 {
            self.dealer_turn = true;
            send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true })
        } else {
            send(Command::Status { hit: true, stand: true, double: true, surrender: false, split: false, new_game: false })
        }
    }
    fn stand(&mut self, _pn: usize, mut send: impl FnMut(Command)) {
        if self.dealer_turn || self.game_over || self.bet == 0 {
            dbg!(self.dealer_turn);
            dbg!(self.game_over);
            dbg!(self.bet);
            return;
        }
        send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true });
        self.dealer_turn = true;
    }
    fn double_down(&mut self, pn: usize, mut send: impl FnMut(Command)) {
        if self.dealer_turn || self.game_over || self.bet == 0 || self.player_hand.cards().len() > 2 {
            return;
        }
        let card = self.draw_card();
        self.player_hand.add_card(card);
        send(Command::PlayerDraw(pn, card));

        send(Command::TakeMoney(self.bet));
        self.bet += self.bet;

        let value = self.player_hand.value();
        send(Command::ValueUpdate(Some(0), value, self.player_hand.is_soft()));
        send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true });
        self.dealer_turn = true;
    }
    fn surrender(&mut self, _pn: usize, mut send: impl FnMut(Command)) {
        if self.dealer_turn || self.game_over || self.bet == 0 || self.player_hand.cards().len() > 2 {
            return;
        }

        let give_back = self.bet / 2;
        self.bet = 0;
        send(Command::SendMoney(give_back));

        send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true });
        self.dealer_turn = true;
    }
    fn split(&mut self, _pn: usize, _send: impl FnMut(Command)) {
        if self.dealer_turn || self.game_over || self.bet == 0 {
            return;
        }
        match self.player_hand.cards() {
            &[c1, c2] if c1.suit_rank().1 == c2.suit_rank().1 => (),
            _ => return,
        }

        eprintln!("unimplemented split cards")
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
    PlayerDraw(usize, Card),
    DeckSize(u8),

    Bet(u32),
    SendMoney(u32),
    TakeMoney(u32),

    // Blackjack
    ValueUpdate(Option<usize>, u8, bool),
    DealerDraw(Card),
    RevealDowns(Card, Vec<Card>),
    DownCard(Card),
    Status{
        hit: bool,
        stand: bool,
        double: bool,
        surrender: bool,
        split: bool,
        new_game: bool,
    },
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
            "BET" => Ok(Command::Bet(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "TAKEMONEY" => Ok(Command::TakeMoney(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "SENDMONEY" => Ok(Command::SendMoney(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "DRAW" => Ok(Command::Draw),
            "REVEALDOWNS" => Ok(Command::RevealDowns(
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?),
                split.map(|c| c.parse().map_err(|_| ()).map(Card::from_u8)).collect_result::<Vec<_>>()?,
            )),
            "PLAYERDRAW" => Ok(Command::PlayerDraw(
                usize::from_str(split.next().ok_or(())?).map_err(|_| ())?,
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?)
            )),
            "DECKSIZE" => Ok(Command::DeckSize(
                split.next().ok_or(())?.parse().map_err(|_| ())?
            )),
            "DEALERDRAW" => Ok(Command::DealerDraw(
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?)
            )),
            "DOWNCARD" => Ok(Command::DownCard(
                Card::from_u8(split.next().ok_or(())?.parse().map_err(|_| ())?),
            )),
            "STAND" => Ok(Command::Stand),
            "HIT" => Ok(Command::Hit),
            "DOUBLEDOWN" => Ok(Command::DoubleDown),
            "SURRENDER" => Ok(Command::Surrender),
            "SPLIT" => Ok(Command::Split),
            "VALUEUPDATE" => {
                let mut iter = split.rev();
                let last = iter.next().ok_or(())?;
                let soft = last == "soft";
                let value = if soft {
                    iter.next().ok_or(())?
                } else {
                    last
                }.parse().map_err(|_| ())?;
                let pn = iter.next().and_then(|pn| pn.parse().ok());
                Ok(Command::ValueUpdate(pn, value, soft))
            }
            "STATUS" => {
                let iter = split;
                let mut hit = false;
                let mut stand = false;
                let mut double = false;
                let mut surrender = false;
                let mut split = false;
                let mut new_game = false;

                for s in iter {
                    match s {
                        "H" => hit = true,
                        "S" => stand = true,
                        "D" => double = true,
                        "U" => surrender = true,
                        "P" => split = true,
                        "N" => new_game = true,
                        _ => return Err(()),
                    }
                }
                Ok(Command::Status { hit, stand, double, surrender, split, new_game })
            }
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
            Command::Bet(i) => write!(f, "BET {i}"),
            Command::TakeMoney(i) => write!(f, "TAKEMONEY {i}"),
            Command::SendMoney(i) => write!(f, "SENDMONEY {i}"),
            Command::Draw => write!(f, "DRAW"),
            Command::PlayerDraw(p, c) => write!(f, "PLAYERDRAW {p} {c}"),
            Command::DeckSize(n) => write!(f, "DECKSIZE {n}"),
            Command::DealerDraw(c) => write!(f, "DEALERDRAW {c}"),
            Command::DownCard(c) => {
                write!(f, "DOWNCARD {c}")?;
                Ok(())
            }
            Command::RevealDowns(c, ps) => {
                write!(f, "REVEALDOWNS {c}")?;
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
            &Command::ValueUpdate(pn, value, soft) => {
                write!(f, "VALUEUPDATE")?;
                if let Some(pn) = pn {
                    write!(f, " {pn}")?;
                }
                write!(f, " {value}")?;
                if soft {
                    write!(f, " soft")?;
                }
                Ok(())
            }
            &Command::Status { hit, stand, double, surrender, split, new_game } => {
                write!(f, "STATUS")?;
                if hit {
                    write!(f, " H")?;
                }
                if stand {
                    write!(f, " S")?;
                }
                if double {
                    write!(f, " D")?;
                }
                if surrender {
                    write!(f, " U")?;
                }
                if split {
                    write!(f, " P")?;
                }
                if new_game {
                    write!(f, " N")?;
                }
                Ok(())
            }
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