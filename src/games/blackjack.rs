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
                        if pid == PlayerId::HOST {
                            cmds.reply(Command::status_new_game());
                        } else {
                            cmds.reply(Command::status_wait());
                        }
                    }
                    Command::Start if pid == PlayerId::HOST && !bets.is_empty() => {
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
                            cmds.send_to(pid, Command::ValueUpdate(Some(pid), hand.value(), hand.is_soft()));
                        }
                        dealer_hand.add_card(self.draw_card());
                        for (hand, &pid) in hands.iter_mut().zip(bets.keys()) {
                            let card = self.draw_card();
                            cmds.broadcast(Command::PlayerDraw(pid, card));
                            let open_hand = Hand::new([card]);
                            cmds.broadcast(Command::ValueUpdate(Some(pid), open_hand.value(), open_hand.is_soft()));
                            hand.add_card(card);
                            cmds.send_to(pid, Command::ValueUpdate(Some(pid), hand.value(), hand.is_soft()));
                        }
                        {
                            let card = self.draw_card();
                            dealer_hand.add_card(card);
                            cmds.broadcast(Command::DealerDraw(card));
                            let open_hand = Hand::new([card]);
                            cmds.broadcast(Command::ValueUpdate(None, open_hand.value(), open_hand.is_soft()));
                        }

                        if dealer_hand.is_natural() {
                            cmds.broadcast(Command::RevealDown(None, dealer_hand.cards()[0]));
                            cmds.broadcast(Command::ValueUpdate(None, dealer_hand.value(), dealer_hand.is_soft()));
                            for (hand, (&pid, &bet)) in hands.iter_mut().zip(bets.iter()) {
                                cmds.broadcast(Command::RevealDown(Some(pid), hand.cards()[0]));
                                cmds.broadcast(Command::ValueUpdate(Some(pid), hand.value(), hand.is_soft()));
                                if hand.is_natural() {
                                    // send bets back to everyone who also got blackjack, otherwise do nothing
                                    cmds.send_to(pid, Command::SendMoney(bet));
                                    cmds.send_to(pid, Command::Draw);
                                } else {
                                    cmds.send_to(pid, Command::Lose);
                                }
                            }
                            self.state = State::NEW;
                        } else {
                            let hands_to_play: Vec<_> = hands.into_iter().zip(bets.iter()).filter_map(|(hand, (&pid, &bet))| {
                                if hand.is_natural() {
                                    cmds.broadcast(Command::RevealDown(Some(pid), hand.cards()[0]));
                                    cmds.broadcast(Command::ValueUpdate(Some(pid), hand.value(), hand.is_soft()));
                                    // send back blackjack win bonus
                                    cmds.send_to(pid, Command::SendMoney(2 * bet + bet / 2));
                                    cmds.send_to(pid, Command::Win);
                                    None
                                } else {
                                    Some(PlayHand {
                                        player: pid,
                                        hand,
                                    })
                                }
                            }).rev().collect();
                            if hands_to_play.is_empty() {
                                cmds.broadcast(Command::RevealDown(None, dealer_hand.cards()[0]));
                                cmds.broadcast(Command::ValueUpdate(None, dealer_hand.value(), dealer_hand.is_soft()));
                                self.state = State::NEW;
                            } else {
                                self.state = State::PlayingInProgress {
                                    hands_to_play,
                                    bets,
                                    dealer_hand,
                                    finished_hands: Vec::new(),
                                };
                                self.notify_new_hand(cmds);
                            }
                        }
                    }
                    _ => return,
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
                let mut split = false;
                let mut split_hand_can_split = false;

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
                        split = true;
                        let (one, two) = match current_hand.hand.cards() {
                            &[one, two] if one.suit_rank().1 == two.suit_rank().1 => (one, two),
                            _ => return
                        };

                        current_hand.hand = Hand::new([one]);
                        let card = draw_card(&mut self.deck, &mut self.dirty_deck);
                        current_hand.hand.add_card(card);
                        cmds.broadcast(Command::PlayerDraw(pid, card));
                        cmds.broadcast(Command::ValueUpdate(Some(pid), current_hand.hand.value(), current_hand.hand.is_soft()));
                        let cards = current_hand.hand.cards();
                        split_hand_can_split = cards[0].suit_rank().1 == cards[1].suit_rank().1;
                        
                        let card = draw_card(&mut self.deck, &mut self.dirty_deck);
                        cmds.broadcast(Command::SplitHandDraw(pid, card));
                        hands_to_play.insert(hands_to_play.len() - 1, PlayHand {
                            player: pid,
                            hand: Hand::new([two, card]),
                        });
                    }
                    _ => return,
                }

                if hand_is_over {
                    cmds.reply(Command::status_wait());
                    let ended_hand = hands_to_play.pop().unwrap();
                    if lost {
                        cmds.reply(Command::Lose);
                    } else {
                        // if the hand lost, we just throw it out.
                        finished_hands.push(PlayedHand {
                            player: ended_hand.player,
                            value: ended_hand.hand.value(),
                            doubled,
                        });
                    }

                    // if there are now no more hands, we finish the game
                    if hands_to_play.is_empty() {
                        self.end_game(cmds);
                    } else {
                        self.notify_new_hand(cmds);
                    }
                } else if split {
                    cmds.reply(Command::status_new(split_hand_can_split));
                } else {
                    cmds.reply(Command::status_mid_hand());
                }
            }
        }
    }
}


fn draw_card(deck: &mut Deck, dirty_deck: &mut bool) -> Card {
    *dirty_deck = true;
    deck.draw_one().unwrap()
}

// TODO: announce bets
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
    fn notify_new_hand(&mut self, mut cmds: CommandQueue) {
        let State::PlayingInProgress { hands_to_play, .. } = &self.state else {
            unreachable!();
        };
        let first_hand_to_play = &hands_to_play[0];
        let cards = first_hand_to_play.hand.cards();
        let split = cards[0].suit_rank().1 == cards[1].suit_rank().1;
        cmds.send_to(first_hand_to_play.player, Command::status_new(split));
    }
    fn end_game(&mut self, mut cmds: CommandQueue) {
        let state = std::mem::replace(&mut self.state, State::AwaitingBets { bets: BTreeMap::new() });
        let State::PlayingInProgress { bets, mut dealer_hand, hands_to_play, finished_hands } = state else {
            unreachable!();
        };
        debug_assert!(hands_to_play.is_empty());
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
                Less => cmds.send_to(player, Command::Lose),
                Equal => {
                    cmds.send_to(player, Command::SendMoney(bet));
                    cmds.send_to(player, Command::Draw);
                }
                Greater => {
                    cmds.send_to(player, Command::SendMoney(2 * bet));
                    cmds.send_to(player, Command::Win);
                }
            }
        }
    }
}
