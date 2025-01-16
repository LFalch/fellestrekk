use rocket::tokio::sync::{RwLock, RwLockReadGuard};
use rocket::State;

use rocket::futures::{SinkExt,StreamExt};
use rocket::tokio::select;
use rocket::tokio::sync::mpsc::error::SendError;
use rocket::tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use rocket::tokio::time::sleep;
use rocket_ws::frame::{CloseFrame, CloseCode};
use rocket_ws::{WebSocket, Channel, stream::DuplexStream, Message};

use std::sync::{Arc, Mutex};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::fmt::{self, Display};
use std::time::Duration;
use std::borrow::Cow;

use rand::{Rng, thread_rng};

use crate::card::Card;
use crate::games::{Blackjack, Game};

type SendResult<T> = Result<T, SendError<Command>>;

type Player = UnboundedSender<Command>;

pub struct Session {
    players: BTreeMap<u32, Player>,
    pub game: Box<dyn Game + Send + Sync>,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(u32);
impl PlayerId {
    pub const HOST: Self = Self(0);
}

impl Session {
    #[inline]
    fn new(host: Player, game: Box<dyn Game + Send + Sync>) -> Self {
        let mut players = BTreeMap::new();
        players.insert(0, host);
        Session {
            players,
            game,
        }
    }
    fn broadcast_command(&self, cmd: Command) -> SendResult<()> {
        for player in self.players.values() {
            player.send(cmd.clone())?;
        }
        Ok(())
    }
    fn send_to(&self, target: PlayerId, cmd: Command) -> SendResult<()> {
        self.players[&target.0].send(cmd)
    }
    /// Whether any player has left (which should end the session)
    fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
    fn join(&mut self, player: Player) -> Option<PlayerId> {
        if self.game.has_space() {
            let new_key = 1 + *self.players.last_key_value().unwrap().0;
            self.players.insert(new_key, player);
            let pid = PlayerId(new_key);
            self.game.join(pid);
            Some(pid)
        } else {
            None
        }
    }
}

fn gen_game_code() -> u16 {
    thread_rng().gen()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameId {
    Blackjack,
    Chatroom,
}

impl FromStr for GameId {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "BLACKJACK" => GameId::Blackjack,
            "CHATROOM" => GameId::Chatroom,
            _ => return Err(()),
        })
    }
}
impl Display for GameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameId::Blackjack => write!(f, "BLACKJACK"),
            GameId::Chatroom => write!(f, "CHATROOM"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Host(GameId),
    Join(u16),
    HostOk(u16),
    JoinOk(u16, Option<String>),
    Start,
    Draw,
    PlayerDraw(PlayerId, Card),
    DeckSize(u8),

    Bet(u32),
    SendMoney(u32),
    TakeMoney(u32),
    // Blackjack
    ValueUpdate(Option<PlayerId>, u8, bool),
    DealerDraw(Card),
    RevealDown(Option<PlayerId>, Card),
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
    ChatMsg(PlayerId, String),
    Chat(String),
    Win,
    Lose,
    Nop
}

impl Command {
    fn into_message(self) -> Message {
        Message::Text(self.to_string())
    }
    pub fn status_new(split: bool) -> Self {
        Self::Status { hit: true, stand: true, double: true, surrender: true, split, new_game: false }
    }
    pub fn status_wait() -> Self {
        Self::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: false }
    }
    pub fn status_new_game() -> Self {
        Self::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true }
    }
    pub fn status_mid_hand() -> Self {
        Self::Status { hit: true, stand: true, double: false, surrender: false, split: false, new_game: false }
    }
}

impl FromStr for Command {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split(' ');
        match split.next().ok_or(())? {
            "HOST" => Ok(Command::Host(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "JOIN" => Ok(Command::Join(u16::from_str_radix(split.next().ok_or(())?, 16).map_err(|_| ())?)),
            "HOST_OK" => Ok(Command::HostOk(u16::from_str_radix(split.next().ok_or(())?, 16).map_err(|_| ())?)),
            "JOIN_OK" => Ok(Command::JoinOk(u16::from_str_radix(split.next().ok_or(())?, 16).map_err(|_| ())?, split.next().map(|s| s.to_owned()))),
            "START" => Ok(Command::Start),
            "BET" => Ok(Command::Bet(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "TAKEMONEY" => Ok(Command::TakeMoney(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "SENDMONEY" => Ok(Command::SendMoney(split.next().and_then(|s| s.parse().ok()).ok_or(())?)),
            "DRAW" => Ok(Command::Draw),
            "REVEALDOWN" => {
                let card = Card::from_u8(split.next_back().ok_or(())?.parse().map_err(|_| ())?);
                let player = if let Some(p) = split.next() {
                    Some(PlayerId(u32::from_str(p).map_err(|_| ())?))
                } else { None };
                Ok(Command::RevealDown(player, card))
            },
            "PLAYERDRAW" => Ok(Command::PlayerDraw(
                PlayerId(u32::from_str(split.next().ok_or(())?).map_err(|_| ())?),
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
                let pn = iter.next().and_then(|pn| pn.parse().map(PlayerId).ok());
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
                PlayerId(u32::from_str(split.next().ok_or(())?).map_err(|_| ())?),
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
            Command::Host(game) => write!(f, "HOST {game}"),
            Command::Join(c) => write!(f, "JOIN {c:X}"),
            Command::HostOk(c) => write!(f, "HOST_OK {c:X}"),
            Command::JoinOk(c, Some(s2)) => write!(f, "JOIN_OK {c:X} {s2}"),
            Command::JoinOk(c, None) => write!(f, "JOIN_OK {c:X}"),
            Command::Start => write!(f, "START"),
            Command::Bet(i) => write!(f, "BET {i}"),
            Command::TakeMoney(i) => write!(f, "TAKEMONEY {i}"),
            Command::SendMoney(i) => write!(f, "SENDMONEY {i}"),
            Command::Draw => write!(f, "DRAW"),
            Command::PlayerDraw(p, c) => write!(f, "PLAYERDRAW {} {c}", p.0),
            Command::DeckSize(n) => write!(f, "DECKSIZE {n}"),
            Command::DealerDraw(c) => write!(f, "DEALERDRAW {c}"),
            Command::DownCard(c) => {
                write!(f, "DOWNCARD {c}")?;
                Ok(())
            }
            Command::RevealDown(p, c) => {
                write!(f, "REVEALDOWN")?;
                if let Some(p) = p {
                    write!(f, " {}", p.0)?;
                }
                write!(f, " {c}")
            }
            Command::Stand => write!(f, "STAND"),
            Command::Hit => write!(f, "HIT"),
            Command::DoubleDown => write!(f, "DOUBLEDOWN"),
            Command::Surrender => write!(f, "SURRENDER"),
            Command::Split => write!(f, "SPLIT"),
            &Command::ValueUpdate(pn, value, soft) => {
                write!(f, "VALUEUPDATE")?;
                if let Some(pid) = pn {
                    write!(f, " {}", pid.0)?;
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
            Command::ChatMsg(p, m) => write!(f, "CHAT_MSG {} {m}", p.0),
            Command::Chat(m) => write!(f, "CHAT {m}"),
            Command::Win => write!(f, "WIN"),
            Command::Lose => write!(f, "LOSE"),
            Command::Nop => write!(f, ""),
        }
    }
}

pub struct CommandQueue<'a> {
    reply_channel: UnboundedSender<Command>,
    send_queue: &'a mut Vec<(Option<PlayerId>, Command)>,
}

impl<'a> CommandQueue<'a> {
    fn new<'b: 'a>(reply_channel: UnboundedSender<Command>, send_queue: &'a mut Vec<(Option<PlayerId>, Command)>) -> CommandQueue<'a> {
        Self {reply_channel, send_queue}
    }
    pub fn reply(&mut self, cmd: Command) {
        self.reply_channel.send(cmd).unwrap();
    }
    pub fn send_to(&mut self, id: PlayerId, cmd: Command) {
        self.send_queue.push((Some(id), cmd));
    }
    pub fn broadcast(&mut self, cmd: Command) {
        self.send_queue.push((None, cmd));
    }
    pub fn reborrow<'b>(&'b mut self) -> CommandQueue<'b> {
        CommandQueue {
            reply_channel: self.reply_channel.clone(),
            send_queue: self.send_queue,
        }
    }
}

#[get("/ws")]
pub fn ws(ws: WebSocket, session_store: &State<SessionStore>) -> Channel<'static> {
    let sessions = session_store.inner().clone();

    ws.channel(move |mut stream: DuplexStream| Box::pin(async move {
        let code: u16;
        let mut assigned_pid: Option<PlayerId> = None;

        let (tx, mut rx) = unbounded_channel();
        let cmd = handle(&mut stream).await?;

        match cmd {
            Command::Join(the_code) => {
                code = the_code;
                let message;
                if let Some(lock) = sessions.get(code).await {
                    let mut session = lock.lock().unwrap();
                    if let Some(pid) = session.join(tx.clone()) {
                        assigned_pid = Some(pid);
                        message = Command::JoinOk(code, None).into_message();
                    } else {
                        message = Message::Close(Some(CloseFrame {
                            code: CloseCode::Again,
                            reason: Cow::Borrowed("Game full")
                        }));
                    }
                } else {
                    message = Message::Close(Some(CloseFrame {
                        code: CloseCode::Policy,
                        reason: Cow::Borrowed("No such game")
                    }));
                }
                stream.send(message).await?;
            }
            Command::Host(_) => {
                assigned_pid = Some(PlayerId::HOST);

                code = loop {
                    let code = gen_game_code();
                    
                    if !sessions.has(code).await {
                        break code;
                    }
                };
                stream.send(Command::HostOk(code).into_message()).await?;
                sessions.add(code.clone(), Session::new(tx.clone(), Box::new(Blackjack::new()))).await;
            }
            c => panic!("didn't except: {:?}", c),
        }

        let Some(pid) = assigned_pid else {
            return Ok(())
        };

        let mut buf = Vec::with_capacity(16);

        let mut broadcast_cmds = Vec::new();
        loop {
            {
                let Some(session_mutex) = sessions.get(code).await else {break;};
                let mut session = session_mutex.lock().unwrap();
                if session.game.tick(CommandQueue::new(tx.clone(), &mut broadcast_cmds)) {
                    continue;
                }
                for (target, cmd) in broadcast_cmds.drain(..) {
                    if let Some(target) = target {
                        session.send_to(target, cmd).unwrap();
                    } else {
                        session.broadcast_command(cmd).unwrap();
                    }
                }
            }
            select! {
                _ = sleep(Duration::from_secs(5)) => {
                    stream.send(Message::Ping(vec![75, 31, 21, 123, 51, 32])).await?;
                }
                n = rx.recv_many(&mut buf, 16) => {
                    if n == 0 {
                        break;
                    }
                    for cmd in buf.drain(..) {
                        stream.feed(cmd.into_message()).await?;
                    }
                    stream.flush().await?;
                }
                cmd = handle(&mut stream) => {
                    let cmd = cmd?;
                    if let Command::Nop = cmd {
                        continue;
                    }

                    let Some(session_mutex) = sessions.get(code).await else {break;};
                    let mut session = session_mutex.lock().unwrap();

                    match cmd {
                        Command::Chat(msg) => {
                            if !msg.is_empty() {
                                session.broadcast_command(Command::ChatMsg(pid, msg)).unwrap();
                            }
                        }
                        cmd => session.game.handle(pid, cmd, CommandQueue::new(tx.clone(), &mut broadcast_cmds)),
                    }
                }
            }
        }

        Ok(())
    }))
}


async fn handle(stream: &mut DuplexStream) -> rocket_ws::result::Result<Command> {
    let Some(message) = stream.next().await else {
        // TODO: probably close the stream
        return Ok(Command::Nop)
    };
    let message = message?;

    match message {
        Message::Close(_) => {
            stream.send(Message::Close(None)).await?;

            return Err(rocket_ws::result::Error::ConnectionClosed)
        }
        Message::Pong(_) => (),
        Message::Ping(vec) => stream.send(Message::Pong(vec)).await?,
        Message::Text(msg) => {
            return Ok(msg.parse().map_err(|()| rocket_ws::result::Error::Utf8)?);
        }
        message => eprintln!("Got unexpected {:?}", message),
    }
    Ok(Command::Nop)
}
 
#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<u16, Mutex<Session>>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        SessionStore {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub async fn add(&self, code: u16, session: Session) {
        let mut sessions = self.sessions.write().await;

        let mut deads = Vec::new();
        for (&code, session) in sessions.iter() {
            match session.lock() {
                Ok(lock) => if lock.is_empty() {
                    deads.push(code);
                }
                _ => deads.push(code),
            }
        }
        for dead in deads {
            sessions.remove(&dead);
        }

        sessions.insert(code, Mutex::new(session));
    }
    pub async fn has(&self, code: u16) -> bool {
        let m = self.sessions.read().await;
        m.contains_key(&code)
    }
    pub async fn get(&self, code: u16) -> Option<RwLockReadGuard<'_, Mutex<Session>>> {
        let m = self.sessions.read().await;
        if let Some(session) = m.get(&code) {
            if !session.is_poisoned() {
                return Some(RwLockReadGuard::map(m, move |m| m.get(&code).unwrap()));
            }
        }
        None
    }
}
