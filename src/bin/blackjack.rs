use std::cmp::Ordering::Equal;
use std::cmp::Ordering::Greater;
use std::cmp::Ordering::Less;
use std::io::Write;
use std::io::stdin;
use std::io::stdout;

#[path = "../card.rs"]
mod card;
#[path = "../dealer.rs"]
mod dealer;
#[path = "../hand.rs"]
mod hand;

use card::*;
use dealer::Dealer;
use crate::hand::Hand;

fn main() {
    let mut winnings = 0;
    let mut deck = Deck::default();
    loop {
        if deck.size() <= 20 {
            deck = Deck::new_standard();
            deck.shuffle();
            println!("Shuffling deck!");
        }
        let Some(()) = blackjack(&mut winnings, Dealer::s17(), &mut deck) else {
            break;
        };
        println!("winnings now: {winnings}");
        println!();
    }
    println!("final winnings: {winnings}");
}

fn blackjack(winnings: &mut i128, dealer: Dealer, deck: &mut Deck) -> Option<()> {
    print!("Input bet: ");
    stdout().flush().ok()?;
    let bet: i128 = input().parse().ok()?;
    println!();

    let hole_player = deck.draw_one().unwrap();
    let hole_dealer = deck.draw_one().unwrap();
    let open_player = deck.draw_one().unwrap();
    let open_dealer = deck.draw_one().unwrap();
    println!("Dealer card: ?? {open_dealer}");
    let mut player_hand = Hand::new([hole_player, open_player]);

    if player_hand.value() == 21 {
        print!("Player hand:");
        player_hand.print();
        let blackjack_winning = (bet * 3) >> 1;
        println!("Blackjack! You win ¤{blackjack_winning}\n");
        *winnings += blackjack_winning;
        return Some(())
    }
    let mut double_down = false;
    'player_hand_loop: loop {
        print!("Player hand:");
        player_hand.print();

        if player_hand.value() >= 21 || double_down {
            break;
        }

        loop {
            if player_hand.cards().len() == 2 {
                print!("s[U]rrender ");
                if player_hand.cards()[0].suit_rank().1 == player_hand.cards()[1].suit_rank().1 {
                    print!("s[P]lit ");
                }
            }
            println!("[H]it [S]tand [D]ouble down");
            print!("Move: ");
            stdout().flush().ok()?;
            match &*input().to_lowercase() {
                "s" => {
                    break 'player_hand_loop;
                }
                "h" => {
                    break;
                }
                "d" => {
                    
                    double_down = true;
                    break;
                }
                "u" => {
                    let half_bet = bet >> 1;
                    println!("Surrender! You get back half your bet (¤{half_bet}).");
                    *winnings -= half_bet;
                    return Some(());
                }
                "p" => {
                    println!("Under development");
                    continue;
                }
                _ => {
                    println!("Invalid move!");
                    continue;
                }
            }
        }

        player_hand.add_card(deck.draw_one().unwrap());
    }
    let bet = if double_down { bet << 1 } else { bet };

    if player_hand.value() > 21 {
        println!("Bust!");
        *winnings -= bet;
        return Some(());
    }
    println!();

    let mut dealer_hand = Hand::new([hole_dealer, open_dealer]);
    while dealer_hand.value() < 17 || (dealer_hand.value() == 17 && dealer_hand.is_soft() && dealer.hit_soft_17) {
        dealer_hand.add_card(deck.draw_one().unwrap());
    }
    print!("Dealer hand:");
    dealer_hand.print();

    if dealer_hand.value() > 21 {
        println!("Dealer bust! You win ¤{bet}!\n");
        *winnings += bet;
        return Some(());
    }
    match player_hand.value().cmp(&dealer_hand.value()) {
        Equal => {
            println!("Draw! You get your bet back");
        }
        Greater => {
            println!("Win! You win ¤{bet}!");
            *winnings += bet;
        }
        Less => {
            println!("You lose! You lose ¤{bet}!");
            *winnings -= bet;
        }
    }
    println!();

    Some(())
}

fn input() -> String {
    let mut line = String::new();
    stdin().read_line(&mut line).unwrap();
    line = line.trim().to_owned();
    line
}
