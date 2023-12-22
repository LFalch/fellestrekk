use crate::card::Card;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hand {
    cards: Vec<Card>,
    dirty: bool,
    soft: bool,
    value: u8,
}

impl Hand {
    pub fn new<const N: usize>(cards: [Card; N]) -> Self {
        Hand {
            cards: cards.to_vec(),
            dirty: true,
            .. Default::default()
        }
    }
    fn clean(&mut self) {
        if self.dirty {
            let (v, s) = value_with_soft(&self.cards);
            self.soft = s;
            self.value = v;

            self.dirty = false;
        }
    }
    pub fn print(&mut self) {
        self.clean();
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
    pub fn is_soft(&mut self) -> bool {
        self.clean();
        self.soft
    }
    pub fn value(&mut self) -> u8 {
        self.clean();
        self.value
    }
    pub fn add_card(&mut self, card: Card) {
        self.cards.push(card);
        self.dirty = true;
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
