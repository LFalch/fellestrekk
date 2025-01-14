use std::cmp::Ordering::{Equal, Greater, Less};

use crate::{card::{Card, Deck}, dealer::Dealer, fellestrekk::{Command, CommandQueue, PlayerId}, hand::{BlackjackExt, Hand}};
use super::Game;

#[derive(Debug, Clone)]
pub struct Blackjack {
    deck: Deck,
    dealer_hand: Hand,
    player_hand: Hand,
    dealer: Dealer,
    turn: Option<usize>,
    bet: u32,

    dirty_deck: bool,
    game_over: bool,

    due_for_tick: bool,
}

impl Game for Blackjack {
    fn has_space(&self) -> bool {
        false
    }
    fn tick(&mut self, mut cmds: CommandQueue) -> bool {
        if (self.game_over && self.bet == 0) || !self.due_for_tick {
            // wait for bet
            return false;
        }
        self.due_for_tick = false;

        if self.turn.is_none() && !self.game_over {
            self.set_due_for_tick();
            while self.dealer.hits(&self.dealer_hand) {
                let card = self.draw_card();
                self.dealer_hand.add_card(card);
                cmds.send(Command::DealerDraw(card));
            }
            self.game_over = true;
            cmds.send(Command::RevealDowns(self.dealer_hand.cards()[0], vec![self.player_hand.cards()[0]]));
            cmds.send(Command::ValueUpdate(None, self.dealer_hand.value(), self.dealer_hand.is_soft()));
            let bet = self.bet;
            self.bet = 0;
            match self.player_hand.cmp(&self.dealer_hand) {
                Less => cmds.send(Command::Lose),
                Greater => {
                    cmds.send(Command::Win);
                    if self.player_hand.is_natural() {
                        // blackjack bonus
                        cmds.send(Command::SendMoney(bet * 2 + bet / 2));
                    } else {
                        cmds.send(Command::SendMoney(bet * 2));
                    }
                }
                Equal => {
                    cmds.send(Command::Draw);
                    cmds.send(Command::SendMoney(bet));
                }
            }
        }

        if self.dirty_deck {
            self.dirty_deck = false;
            cmds.send(Command::DeckSize(self.deck.size() as u8));
        }
        true
    }
    fn handle(&mut self, pid: PlayerId, cmd: Command, mut cmds: CommandQueue) {
        match cmd {
            Command::Bet(bet) => self.bet(pid, bet, cmds),
            Command::Hit => self.hit(pid, cmds),
            Command::Stand => self.stand(pid, cmds),
            Command::DoubleDown => self.double_down(pid, cmds),
            Command::Surrender => self.surrender(pid, cmds),
            Command::Split => self.split(pid, cmds),
            Command::Start => {
                if self.game_over && self.bet != 0 {
                    self.set_due_for_tick();
                    cmds.send(Command::Start);
                    self.game_over = false;
                    self.turn = Some(0);
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

                    cmds.send(Command::ValueUpdate(None, hand.value(), hand.is_soft()));
                    cmds.send(Command::DownCard(down_player));
                    cmds.send(Command::PlayerDraw(PlayerId::HOST, open_player));
                    cmds.send(Command::DealerDraw(self.dealer_hand.cards()[1]));

                    let split = self.player_hand.cards()[0].suit_rank().1 == self.player_hand.cards()[1].suit_rank().1;

                    cmds.send(Command::ValueUpdate(Some(PlayerId::HOST), self.player_hand.value(), self.player_hand.is_soft()));
                    if self.player_hand.is_natural() {
                        self.stand(PlayerId::HOST, cmds.reborrow());
                    } else {
                        cmds.send(Command::Status { hit: true, stand: true, double: true, surrender: true, split, new_game: false });
                    }
                }
            }
            _ => (),
        }
    }
}

// TODO: move blackjack game into its own module
// TODO: implement multiple hands (and multiple players)
// TODO: make splits work
impl Blackjack {
    pub fn new() -> Blackjack {
        Blackjack {
            deck: Deck::empty(),
            dealer_hand: Hand::new([]),
            player_hand: Hand::new([]),
            dealer: Dealer::h17(),
            dirty_deck: true,
            game_over: true,
            turn: None,
            bet: 0,

            due_for_tick: true,
        }
    }
    fn set_due_for_tick(&mut self) {
        self.due_for_tick = true;
    }
    fn draw_card(&mut self) -> Card {
        self.set_due_for_tick();
        self.dirty_deck = true;
        self.deck.draw_one().unwrap()
    }
    fn bet(&mut self, _pn: PlayerId, bet: u32, mut cmds: CommandQueue) {
        self.set_due_for_tick();
        if self.bet != 0 {
            return;
        }
        cmds.send(Command::TakeMoney(bet));
        self.bet = bet;
    }
    fn hit(&mut self, pn: PlayerId, mut cmds: CommandQueue) {
        self.set_due_for_tick();
        if self.turn.is_none() || self.game_over || self.bet == 0 {
            return;
        }
        let card = self.draw_card();
        self.player_hand.add_card(card);
        cmds.send(Command::PlayerDraw(pn, card));

        let value = self.player_hand.value();
        cmds.send(Command::ValueUpdate(Some(PlayerId::HOST), value, self.player_hand.is_soft()));
        if self.player_hand.value() > 21 {
            self.turn = None;
            cmds.send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true })
        } else {
            cmds.send(Command::Status { hit: true, stand: true, double: true, surrender: false, split: false, new_game: false })
        }
    }
    fn stand(&mut self, _pn: PlayerId, mut cmds: CommandQueue) {
        self.set_due_for_tick();
        if self.turn.is_none() || self.game_over || self.bet == 0 {
            return;
        }
        cmds.send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true });
        self.turn = None;
    }
    fn double_down(&mut self, pn: PlayerId, mut cmds: CommandQueue) {
        self.set_due_for_tick();
        if self.turn.is_none() || self.game_over || self.bet == 0 || self.player_hand.cards().len() > 2 {
            return;
        }
        let card = self.draw_card();
        self.player_hand.add_card(card);
        cmds.send(Command::PlayerDraw(pn, card));

        cmds.send(Command::TakeMoney(self.bet));
        self.bet += self.bet;

        let value = self.player_hand.value();
        cmds.send(Command::ValueUpdate(Some(PlayerId::HOST), value, self.player_hand.is_soft()));
        cmds.send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true });
        self.turn = None;
    }
    fn surrender(&mut self, _pn: PlayerId, mut cmds: CommandQueue) {
        self.set_due_for_tick();
        if self.turn.is_none() || self.game_over || self.bet == 0 || self.player_hand.cards().len() > 2 {
            return;
        }

        let give_back = self.bet / 2;
        self.bet = 0;
        cmds.send(Command::SendMoney(give_back));

        cmds.send(Command::Status { hit: false, stand: false, double: false, surrender: false, split: false, new_game: true });
        self.turn = None;
    }
    fn split(&mut self, _pn: PlayerId, _cmds: CommandQueue) {
        self.set_due_for_tick();
        if self.turn.is_none() || self.game_over || self.bet == 0 {
            return;
        }
        match self.player_hand.cards() {
            &[c1, c2] if c1.suit_rank().1 == c2.suit_rank().1 => (),
            _ => return,
        }

        eprintln!("unimplemented split cards")
    }
}
