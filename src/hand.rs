use std::cmp::Ordering;

use crate::card::Card;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hand {
    cards: Vec<Card>,
    soft: bool,
    value: u8,
}

impl Hand {
    pub fn new<const N: usize>(cards: [Card; N]) -> Self {
        let mut h = Hand {
            cards: cards.to_vec(),
            .. Default::default()
        };
        h.update();
        h
    }
    fn update(&mut self) {
        let (v, s) = value_with_soft(&self.cards);
        self.soft = s;
        self.value = v;
    }
    pub fn print(&self) {
        for card in &self.cards {
            print!(" {card}");
        }
        println!();
        print!("Value: {}", self.value);
        if self.soft {
            print!(" or {}", self.value - 10);
        }
        println!();
    }
    pub fn is_soft(&self) -> bool {
        self.soft
    }
    pub fn value(&self) -> u8 {
        self.value
    }
    pub fn add_card(&mut self, card: Card) {
        self.cards.push(card);
        self.update();
    }
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }
}

fn value_with_soft(hand: &[Card]) -> (u8, bool) {
    let mut ace = false;
    let mut hand_value = 0;
    for card in hand {
        let card_value = card.suit_rank().1 as u8 + 1;
        if card_value == 1 {
            ace = true;
        }
        if card_value >= 10 {
            hand_value += 10;
        } else {
            hand_value += card_value;
        }
    }

    if ace && hand_value <= 11 {
        (hand_value + 10, true)
    } else {
        (hand_value, false)
    }
}

pub trait BlackjackExt {
    fn cmp(&self, other: &Self) -> Ordering;
    fn is_bust(&self) -> bool;
    fn is_natural(&self) -> bool;
}

impl BlackjackExt for Hand {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_natural() {
            return if other.is_natural() {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }
        if self.is_bust() {
            return Ordering::Less
        } else if other.is_bust() {
            return Ordering::Greater
        }
        self.value().cmp(&other.value())
    }
    #[inline]
    fn is_bust(&self) -> bool {
        self.value() > 21
    }
    #[inline]
    fn is_natural(&self) -> bool {
        self.cards().len() == 2 && self.value() == 21
    }
}