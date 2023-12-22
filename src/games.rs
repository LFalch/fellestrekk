#[derive(Debug, Clone)]
pub enum Game {
    Blackjack,
    ChatRoom,
}
/*
impl Game {
    fn new(stor: bool) -> Game {
        let size = if stor { 13 } else { 11 };
        let last = size - 1;
        let mid = size / 2;
        let konge = Pos(mid, mid);
        let mut hirdmenn = Vec::with_capacity(12);
        let mut aatakarar = Vec::with_capacity(24);

        for i in -2..=2 {
            aatakarar.push(Pos(mid+i, 0));
            aatakarar.push(Pos(mid+i, last));
            aatakarar.push(Pos(0, mid+i));
            aatakarar.push(Pos(last, mid+i));

            let k = 2 - i.abs();

            for j in -k..=k {
                if i == 0 && j == 0 {
                    aatakarar.push(Pos(mid + i, 1));
                    aatakarar.push(Pos(mid + i, last - 1));
                    aatakarar.push(Pos(1, mid + i));
                    aatakarar.push(Pos(last - 1, mid + i));
                } else {
                    hirdmenn.push(Pos(i+mid, j+mid));
                }
            }
        }

        Game {
            size,
            turn: Team::Aatak,
            konge,
            hirdmenn,
            aatakarar
        }
    }
    fn find(&self, x: i8, y: i8) -> Option<PieceOnBoard> {
        if self.konge.at(x, y) {
            Some(PieceOnBoard::Konge)
        } else {
            for (i, aatakar) in self.aatakarar.iter().enumerate() {
                if aatakar.at(x, y) {
                    return Some(PieceOnBoard::Aatakar(i))
                }
            }
            for (i, hirdmann) in self.hirdmenn.iter().enumerate() {
                if hirdmann.at(x, y) {
                    return Some(PieceOnBoard::Hirdmann(i))
                }
            }
            None
        }
    }
    #[allow(dead_code)]
    fn get_pos(&self, piece: PieceOnBoard) -> Pos {
        match piece {
            PieceOnBoard::Konge => self.konge,
            PieceOnBoard::Hirdmann(i) => self.hirdmenn[i],
            PieceOnBoard::Aatakar(i) => self.aatakarar[i]
        }
    }
    fn get_mut_pos(&mut self, piece: PieceOnBoard) -> &mut Pos {
        match piece {
            PieceOnBoard::Konge => &mut self.konge,
            PieceOnBoard::Hirdmann(i) => &mut self.hirdmenn[i],
            PieceOnBoard::Aatakar(i) => &mut self.aatakarar[i]
        }
    }
    #[inline]
    fn can_pass(&self, x: i8, y: i8) -> bool {
        self.find(x, y).is_none()
    }
    fn can_go(&self, piece: PieceOnBoard, x: i8, y: i8) -> bool {
        self.can_pass(x, y) &&
        if let PieceOnBoard::Konge = piece {
            true
        } else {
            !self.is_castle(x, y)
        }
    }
    #[inline]
    fn is_castle(&self, x: i8, y: i8) -> bool {
        self.is_escape_castle(x, y) || self.is_middle_castle(x, y)
    }
    #[inline]
    fn is_middle_castle(&self, x: i8, y: i8) -> bool {
        let mid = self.size / 2;

        x == mid && y == mid
    }
    #[inline]
    fn is_escape_castle(&self, x: i8, y: i8) -> bool {
        let last = self.size - 1;

        (x == 0 || x == last) && (y == 0 || y == last)
    }
    #[inline]
    fn out_of_bounds(&self, x: i8, y: i8) -> bool {
        x < 0 || x >= self.size || y < 0 || y >= self.size
    }
    fn do_move(&mut self, x: i8, y: i8, dx: i8, dy: i8, team: Team) -> Vec<Command> {
        let mut cmds = Vec::with_capacity(4);

        let dest_x = x + dx;
        let dest_y = y + dy;

        if self.out_of_bounds(dest_x, dest_y) {
            return cmds;
        }

        if let Some(piece) = self.find(x, y) {
            if piece.team() == team && team == self.turn && self.can_go(piece, dest_x, dest_y) {
                let can_pass = match (dx, dy) {
                    (0, 1 ..= 127) => (y+1..=dest_y).map(|y| (dest_x, y)).all(|(x, y)| self.can_pass(x, y)),
                    (0, -128 ..= -1) => (dest_y..y).map(|y| (dest_x, y)).all(|(x, y)| self.can_pass(x, y)),
                    (1 ..= 127, 0) => (x+1..=dest_x).map(|x| (x, dest_y)).all(|(x, y)| self.can_pass(x, y)),
                    (-128 ..= -1, 0) => (dest_x..x).map(|x| (x, dest_y)).all(|(x, y)| self.can_pass(x, y)),
                    (0, 0) | (_, _) => return cmds,
                };

                if can_pass {
                    cmds.push(Command::Move(x, y, dx, dy));
                    let dest = Pos(dest_x, dest_y);
                    *self.get_mut_pos(piece) = dest;

                    for (x, y) in dest.surround() {
                        if let Some(threatened_piece) = self.find(x, y) {
                            if threatened_piece.team() != team {
                                let (x2, y2) = (2 * x - dest.0, 2 * y - dest.1);
                                let other_side = self.find(x2, y2).map(|p| p.team());
                                
                                if Some(team) == other_side || (team == Team::Hirdi && (self.is_castle(x2, y2))) {
                                    let dead = match threatened_piece {
                                        PieceOnBoard::Aatakar(i) => self.aatakarar.remove(i),
                                        PieceOnBoard::Hirdmann(i) => self.hirdmenn.remove(i),
                                        PieceOnBoard::Konge => continue,
                                    };
                                    debug_assert_eq!(dead, Pos(x, y));
                                    cmds.push(Command::Delete(x, y));
                                }
                            }
                        }
                    }

                    self.turn.switch();
                }
            }
        }

        cmds
    }
    fn who_has_won(&self) -> Option<Team> {
        let Pos(kx, ky) = self.konge;

        if self.is_escape_castle(kx, ky) {
            Some(Team::Hirdi)
        } else {
            let king_captured = Pos(kx, ky).surround().all(|(x, y)| {
                self.out_of_bounds(x, y) ||
                self.is_middle_castle(x, y) ||
                self.find(x, y).map(|p| p.team()) == Some(Team::Aatak)
            });
            if king_captured {
                Some(Team::Aatak)
            } else {
                None
            }
        }
    }
}
*/
