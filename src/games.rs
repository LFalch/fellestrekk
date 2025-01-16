use crate::fellestrekk::{Command, CommandQueue, PlayerId};

pub trait Game {
    fn join(&mut self, _pid: PlayerId) {}
    fn leave(&mut self, _pid: PlayerId) {}
    fn has_space(&self) -> bool {
        false
    }
    fn tick(&mut self, _cmds: CommandQueue) -> bool {
        false
    }
    fn handle(&mut self, _pid: PlayerId, _cmd: Command, _cmds: CommandQueue) {}
}

pub struct Empty;
impl Game for Empty {}

mod blackjack;
pub use self::blackjack::*;
