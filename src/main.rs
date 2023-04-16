use pgn_reader::{BufferedReader, Nag, Outcome, RawComment, RawHeader, SanPlus, Skip, Visitor};
use regex::Regex;
use shakmaty::{Chess, Color, Position, Role};
use std::fs;
use std::io;
use std::time::Instant;
use polars_core::prelude::*;

// struct MoveCounter {
//     moves: usize,
// }

// impl MoveCounter {
//     fn new() -> MoveCounter {
//         MoveCounter { moves: 0 }
//     }
// }

// impl Visitor for MoveCounter {
//     type Result = usize;

//     fn begin_game(&mut self) {
//         self.moves = 0;
//     }

//     fn san(&mut self, _san_plus: SanPlus) {
//         self.moves += 1;
//     }

//     fn begin_variation(&mut self) -> Skip {
//         Skip(true) // stay in the mainline
//     }

//     fn end_game(&mut self) -> Self::Result {
//         self.moves
//     }
// }

#[derive(Clone, Debug)]
struct BitBoard {
    pawn: u64,
    bishop: u64,
    knight: u64,
    rook: u64,
    queen: u64,
    king: u64,
    white: u64,
    black: u64,
}

#[derive(Clone, Debug)]
struct GameInfo{
	move_classes: Vec<u8>,
    move_class_idx: Vec<i32>,
    evals: Vec<f32>,
    evals_idx: Vec<i32>,
    mate_evals: Vec<i32>,
    mate_evals_idx: Vec<i32>,
    white_elo: i32,
    black_elo: i32,
    bitboards: Vec<BitBoard>,
}

#[derive(Clone, Debug)]
struct BoardEvaluator {
    move_classes: Vec<Nag>,
    move_class_index: Vec<i32>,
    evals: Vec<f32>,
    evals_idx: Vec<i32>,
    mate_evals: Vec<i32>,
    mate_evals_idx: Vec<i32>,
    white_elo: i32,
    black_elo: i32,
    is_blitz: bool,
    index: i32,
    current_pos: Chess,
    bitboards: Vec<BitBoard>,
}

impl BoardEvaluator {
    fn new() -> BoardEvaluator {
        BoardEvaluator {
            move_classes: Vec::<Nag>::new(),
            move_class_index: Vec::<i32>::new(),
            evals: Vec::<f32>::new(),
            evals_idx: Vec::<i32>::new(),
            mate_evals: Vec::<i32>::new(),
            mate_evals_idx: Vec::<i32>::new(),
            white_elo: 0,
            black_elo: 0,
            is_blitz: false,
            index: 0,
            current_pos: Chess::default(),
            bitboards: Vec::<BitBoard>::new(),
        }
    }
}

impl Visitor for BoardEvaluator {
    type Result = GameInfo;

    fn begin_game(&mut self) {
        self.move_classes.clear();
        self.move_class_index.clear();
        self.evals.clear();
        self.evals_idx.clear();
        self.mate_evals.clear();
        self.mate_evals_idx.clear();
        self.white_elo = 0;
        self.black_elo = 0;
        self.is_blitz = false;
        self.index = 0;
        self.current_pos = Chess::default();
        self.bitboards.clear();
    }

    fn nag(&mut self, _nag: Nag) {
        self.move_classes.push(_nag);
        self.move_class_index.push(self.index - 1); // called after san(), so index-1
    }

    // for each header
    fn header(&mut self, _key: &[u8], _value: RawHeader<'_>) {
        let header: String = String::from_utf8(_key.to_vec()).unwrap().to_lowercase();
        let value: String = String::from_utf8(_value.as_bytes().to_vec())
            .unwrap()
            .to_lowercase();
        if header == "event" {
            if value.contains("blitz") {
                self.is_blitz = true;
            }
        } else if header == "whiteelo" {
            self.white_elo = value.parse::<i32>().unwrap()
        } else if header == "blackelo" {
            self.black_elo = value.parse::<i32>().unwrap()
        }
    }

    // for each move in game
    fn san(&mut self, san_plus: SanPlus) {
        self.index += 1;

        // play the move and get board position
        if let Ok(m) = san_plus.san.to_move(&self.current_pos) {
            self.current_pos.play_unchecked(&m);

            let (role, colour) = self.current_pos.board().to_owned().into_bitboards();

            let bitboard = BitBoard {
                pawn: role.get(Role::Pawn).0,
                bishop: role.get(Role::Bishop).0,
                knight: role.get(Role::Knight).0,
                rook: role.get(Role::Rook).0,
                queen: role.get(Role::Queen).0,
                king: role.get(Role::King).0,
                white: colour.get(Color::White).0,
                black: colour.get(Color::Black).0,
            };

            self.bitboards.push(bitboard);
        }
    }

    fn outcome(&mut self, _outcome: Option<Outcome>) {}

    // skip game if header contained stuff we don't want
    fn end_headers(&mut self) -> Skip {
        Skip(self.is_blitz)
    }

    // for each comment
    fn comment(&mut self, _comment: RawComment<'_>) {
        let float_eval_regex = Regex::new(r"\[%eval\s(\d+(\.\d+)?)\]").unwrap();
        let mate_eval_regex = Regex::new(r"\[%eval\s#(\d+)\]").unwrap();

        let the_comment: String = String::from_utf8(_comment.as_bytes().to_vec())
            .unwrap()
            .to_lowercase();

        // for a float eval
        if let Some(capture) = float_eval_regex.captures(&the_comment) {
            let value = capture.get(1).unwrap().as_str();
            let number = value.parse::<f32>().unwrap();
            // println!("The eval value is: {}", number);

            self.evals.push(number);
            self.evals_idx.push(self.index);
        }

        // for a mate in x eval
        if let Some(capture) = mate_eval_regex.captures(&the_comment) {
            let value = capture.get(1).unwrap().as_str();
            let number = value.parse::<i32>().unwrap();

            // println!("The eval value is: [mate in] {}", number);
            self.mate_evals.push(number);
            self.mate_evals_idx.push(self.index);
        }
    }

    fn begin_variation(&mut self) -> Skip {
        Skip(true) // stay in the mainline
    }

    fn end_game(&mut self) -> Self::Result {
		let mut move_classes = Vec::<u8>::new();
		for nag in &self.move_classes {
			move_classes.push(nag.0);
		}

		GameInfo {
			move_classes: move_classes.clone(),
			move_class_idx: self.move_class_index.clone(),
			evals: self.evals.clone(),
			evals_idx: self.evals_idx.clone(),
			mate_evals: self.mate_evals.clone(),
			mate_evals_idx: self.mate_evals_idx.clone(),
			white_elo: self.white_elo.clone(),
			black_elo: self.black_elo.clone(),
			bitboards: self.bitboards.clone(),
		}
    }
}

fn main() -> io::Result<()> {
    let pgn: &str = &fs::read_to_string("../lichess_db_standard_rated_2023-03.pgn")
        .expect("Error reading file");
    // let pgn: &str = &fs::read_to_string("Testgame.pgn").expect("Error reading file");

    let mut reader = BufferedReader::new_cursor(&pgn[..]);

    // let mut counter = MoveCounter::new();
    let mut evaluator = BoardEvaluator::new();

    let now = Instant::now();
    let mut total_count = 0;
	let mut eval_count = 0;

	let mut games = Vec::<GameInfo>::new();

	while reader.has_more().unwrap() {
        let board = reader.read_game(&mut evaluator)?;

        let unwrapped_board = board.unwrap();

        if unwrapped_board.evals.len() > 0 {
			eval_count += 1;

			games.push(unwrapped_board);

			if eval_count % 100 == 0 {
				println!("Eval count {}", eval_count);
			}
        }
        total_count += 1;

		

        if total_count > 1000 {
            break;
        }
    }

	let move_classes_series: Series = Series::new(
        "move_classes",
        games.iter().map(|g| g.move_classes.iter().collect::<Series>()).collect::<Vec<_>>()
	);

	println!("{:>}", move_classes_series);
	
    let elapsed_time = now.elapsed().as_secs_f64();

    println!(
        "Processed {} games in {} s , {} [boards/s] {} [evaluated boards/s]",
        total_count,
        elapsed_time,
        f64::from(total_count) / elapsed_time,
		f64::from(eval_count) / elapsed_time,
    );

    Ok(())
}
