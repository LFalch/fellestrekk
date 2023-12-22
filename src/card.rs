use std::fmt::{Display, self};

use rand::seq::SliceRandom;
use rand::thread_rng;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Card(u8);

impl Card {
    pub fn new((suit, rank): (Suit, Rank)) -> Self {
        Card(suit as u8 * 13 + rank as u8)
    }
    pub fn suit_rank(self) -> (Suit, Rank) {
        let Card(v) = self;
        let (vs, vr) = (v / 13, v % 13);

        let suit = match vs {
            0 => Suit::Clubs,
            1 => Suit::Hearts,
            2 => Suit::Spades,
            3 => Suit::Diamonds,
            _ => unreachable!(),
        };
        let rank = match vr {
            12 => Rank::King,
            11 => Rank::Queen,
            10 => Rank::Jack,
            9 => Rank::Ten,
            8 => Rank::Nine,
            7 => Rank::Eight,
            6 => Rank::Seven,
            5 => Rank::Six,
            4 => Rank::Five,
            3 => Rank::Four,
            2 => Rank::Three,
            1 => Rank::Two,
            0 => Rank::Ace,
            _ => unreachable!(),
        };

        (suit, rank)
    }
    pub fn into_u8(self) -> u8 {
        self.0
    }
    pub fn from_u8(n: u8) -> Self {
        Self(n)
    }
}

impl Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (s, r) = self.suit_rank();
        match s {
            Suit::Clubs => write!(f, "♣")?,
            Suit::Hearts => write!(f, "♥")?,
            Suit::Spades => write!(f, "♠")?,
            Suit::Diamonds => write!(f, "♦")?,
        }
        match r {
            Rank::King => write!(f, "K"),
            Rank::Queen => write!(f, "Q"),
            Rank::Jack => write!(f, "J"),
            Rank::Ten => write!(f, "10"),
            Rank::Nine => write!(f, "9"),
            Rank::Eight => write!(f, "8"),
            Rank::Seven => write!(f, "7"),
            Rank::Six => write!(f, "6"),
            Rank::Five => write!(f, "5"),
            Rank::Four => write!(f, "4"),
            Rank::Three => write!(f, "3"),
            Rank::Two => write!(f, "2"),
            Rank::Ace => write!(f, "A"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Suit {
    Clubs = 0,
    Hearts = 1,
    Spades = 2,
    Diamonds = 3,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Rank {
    King = 12,
    Queen = 11,
    Jack = 10,
    Ten = 9,
    Nine = 8,
    Eight = 7,
    Seven = 6,
    Six = 5,
    Five = 4,
    Four = 3,
    Three = 2,
    Two = 1,
    Ace = 0,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn new_standard() -> Self {
        Self {
            cards: (0..52).map(Card).collect(),
        }
    }
    pub const fn empty() -> Self {
        Self {
            cards: Vec::new(),
        }
    }
    pub fn shuffle(&mut self) {
        self.cards.shuffle(&mut thread_rng())
    }
    pub fn draw_one(&mut self) -> Option<Card> {
        self.cards.pop()
    }
    pub fn put_in_back(&mut self, card: Card) {
        self.cards.insert(0, card);
    }
    pub fn size(&self) -> usize {
        self.cards.len()
    }
}
