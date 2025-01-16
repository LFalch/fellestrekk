use std::{cmp::Ordering::{Equal, Greater, Less}, collections::BTreeMap};

use crate::{card::{Card, Deck}, dealer::Dealer, fellestrekk::{Command, CommandQueue, PlayerId}, hand::{BlackjackExt, Hand}};
use super::Game;

type Money = u32;

#[derive(Debug, Clone)]
struct PlayHand {
    player: PlayerId,
    hand: Hand,
}
#[derive(Debug, Clone)]
struct PlayedHand {
    player: PlayerId,
    doubled: bool,
    value: u8,
}
#[derive(Debug, Clone)]
enum State {
    AwaitingBets {
        bets: BTreeMap<PlayerId, Money>,
    },
    PlayingInProgress {
        bets: BTreeMap<PlayerId, Money>,
        // should not be a blackjack, as it should be resolved immediately
        dealer_hand: Hand,
        // should not include blackjacks as they should be paid out immediately
        // stored in reverse order of play in order to be able to pop them
        hands_to_play: Vec<PlayHand>,
        // should not include blackjacks as they come from `hands_to_play`
        finished_hands: Vec<PlayedHand>
    },
}
impl State {
    const NEW: Self = State::AwaitingBets { bets: BTreeMap::new() };
}

#[derive(Debug, Clone)]
pub struct Blackjack {
    deck: Deck,
    dealer: Dealer,

    state: State,

    dirty_deck: bool,
}

impl Game for Blackjack {
    fn has_space(&self) -> bool {
        true
    }
    fn tick(&mut self, mut cmds: CommandQueue) -> bool {
        if self.dirty_deck {
            self.dirty_deck = false;
            cmds.broadcast(Command::DeckSize(self.deck.size() as u8));
        }
        false
    }
    fn handle(&mut self, pid: PlayerId, cmd: Command, mut cmds: CommandQueue) {
        match &mut self.state {
            State::AwaitingBets { bets } => {
                match cmd {
                    Command::Bet(bet) => if bet != 0 {
                        bets.insert(pid, bet);
                        cmds.reply(Command::TakeMoney(bet));
                    }
                    Command::Start if pid == PlayerId::HOST => {
                        let bets = std::mem::replace(bets, BTreeMap::new());
                        cmds.broadcast(Command::Start);
                        if self.deck.size() < 20 {
                            self.deck = Deck::new_standard();
                            self.deck.shuffle();
                            cmds.broadcast(Command::DeckSize(self.deck.size() as u8));
                        }

                        let mut hands = vec![Hand::new([]); bets.len()];
                        let mut dealer_hand = Hand::new([]);

                        for (hand, &pid) in hands.iter_mut().zip(bets.keys()) {
                            let card = self.draw_card();
                            cmds.send_to(pid, Command::DownCard(card));
                            hand.add_card(card);
                        }
                        dealer_hand.add_card(self.draw_card());
                        for (hand, &pid) in hands.iter_mut().zip(bets.keys()) {
                            let card = self.draw_card();
                            cmds.broadcast(Command::PlayerDraw(pid, card));
                            hand.add_card(card);
                        }
                        {
                            let card = self.draw_card();
                            dealer_hand.add_card(card);
                            cmds.broadcast(Command::DealerDraw(card));
                        }

                        // TODO: send updated values correctly. let the client handle it itself?

                        if dealer_hand.is_natural() {
                            for (hand, (&pid, &bet)) in hands.iter_mut().zip(bets.iter()) {
                                if hand.is_natural() {
                                    // send bets back to everyone who also got blackjack, otherwise do nothing
                                    cmds.send_to(pid, Command::SendMoney(bet))
                                }
                            }
                            self.state = State::NEW;
                        } else {
                            self.state = State::PlayingInProgress {
                                hands_to_play: hands.into_iter().zip(bets.iter()).filter_map(|(hand, (&pid, &bet))| {
                                    if hand.is_natural() {
                                        // send back blackjack win bonus
                                        cmds.send_to(pid, Command::SendMoney(2 * bet + bet / 2));
                                        None
                                    } else {
                                        Some(PlayHand {
                                            player: pid,
                                            hand,
                                        })
                                    }
                                }).rev().collect(),
                                bets,
                                dealer_hand,
                                finished_hands: Vec::new(),
                            };
                        }
                    }
                    _ => (),
                }
            }
            State::PlayingInProgress {
                bets, dealer_hand: _,
                hands_to_play, finished_hands
            } => {
                let Some(current_hand) = hands_to_play.last_mut() else {
                    return
                };
                if current_hand.player != pid {
                    return;
                }

                let mut hand_is_over = false;
                let mut lost = false;
                let mut doubled = false;

                match cmd {
                    Command::Hit => {
                        let card = draw_card(&mut self.deck, &mut self.dirty_deck);
                        current_hand.hand.add_card(card);
                        cmds.broadcast(Command::PlayerDraw(pid, card));

                        let value = current_hand.hand.value();
                        cmds.broadcast(Command::ValueUpdate(Some(pid), value, current_hand.hand.is_soft()));
                        if value > 21 {
                            hand_is_over = true;
                            lost = true;
                        }
                    }
                    Command::Stand => {
                        hand_is_over = true;
                    }
                    Command::Surrender => {
                        if current_hand.hand.cards().len() == 2 {
                            hand_is_over = true;
                            lost = true;
                            // Send half the bet back before removing the hand from play
                            cmds.reply(Command::SendMoney(bets[&pid] / 2));
                        }
                    }
                    Command::DoubleDown => {
                        if current_hand.hand.cards().len() == 2 {
                            hand_is_over = true;
                            doubled = true;
                            cmds.reply(Command::TakeMoney(bets[&pid]));

                            let card = draw_card(&mut self.deck, &mut self.dirty_deck);
                            current_hand.hand.add_card(card);
                            cmds.broadcast(Command::PlayerDraw(pid, card));

                            
                            let value = current_hand.hand.value();
                            cmds.broadcast(Command::ValueUpdate(Some(pid), value, current_hand.hand.is_soft()));
                            lost = value > 21;
                        }
                    }
                    Command::Split => {
                        // do split
                    }
                    _ => (),
                }

                if hand_is_over {
                    let ended_hand = hands_to_play.pop().unwrap();
                    if !lost {
                        // if the hand lost, we just throw it out. who cares?
                        finished_hands.push(PlayedHand {
                            player: ended_hand.player,
                            value: ended_hand.hand.value(),
                            doubled,
                        });
                    }

                    // if there are now no more hands, we finish the game
                    if hands_to_play.is_empty() {
                        self.end_game(cmds);
                    }
                }
            }
        }
    }
}


fn draw_card(deck: &mut Deck, dirty_deck: &mut bool) -> Card {
    *dirty_deck = true;
    deck.draw_one().unwrap()
}

// TODO: make splits work
// TODO: announce bets
// TODO: send back the status of the game to players (and what they can do)
impl Blackjack {
    pub fn new() -> Blackjack {
        Blackjack {
            deck: Deck::empty(),
            dealer: Dealer::h17(),
            state: State::NEW,
            dirty_deck: true,
        }
    }
    fn draw_card(&mut self) -> Card {
        draw_card(&mut self.deck, &mut self.dirty_deck)
    }
    fn end_game(&mut self, mut cmds: CommandQueue) {
        let state = std::mem::replace(&mut self.state, State::AwaitingBets { bets: BTreeMap::new() });
        let State::PlayingInProgress { bets, mut dealer_hand, hands_to_play, finished_hands } = state else {
            unreachable!();
        };
        debug_assert!(hands_to_play.is_empty());
        // TODO: reveal down cards of player hands as they are played
        cmds.broadcast(Command::RevealDown(None, dealer_hand.cards()[0]));
        cmds.broadcast(Command::ValueUpdate(None, dealer_hand.value(), dealer_hand.is_soft()));
        while self.dealer.hits(&dealer_hand) {
            let card = self.draw_card();
            dealer_hand.add_card(card);
            cmds.broadcast(Command::DealerDraw(card));
            cmds.broadcast(Command::ValueUpdate(None, dealer_hand.value(), dealer_hand.is_soft()));
        }
        let final_dealer_value = dealer_hand.value();
        let dealer_bust = dealer_hand.is_bust();
        for PlayedHand { player, value, doubled } in finished_hands {
            let outcome = if dealer_bust { Greater } else {
                value.cmp(&final_dealer_value)
            };
            let bet = if doubled {
                2 * bets[&player]
            } else {
                bets[&player]
            };
            match outcome {
                Less => (),
                Equal => cmds.send_to(player, Command::SendMoney(bet)),
                Greater => cmds.send_to(player, Command::SendMoney(2 * bet)),
            }
        }
    }
}
