use std::io;
use std::fs;
use pgn_reader::{Visitor, Skip, BufferedReader, SanPlus, Nag, RawComment, RawHeader, Outcome};
use regex::Regex;

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
struct BoardEvaluator{
	move_classes: Vec<Nag>,
	move_index: Vec<i32>,
	white_elo: i32,
	black_elo: i32,
	is_blitz: bool,
	index: i32
}

impl BoardEvaluator {
	fn new() -> BoardEvaluator {
		BoardEvaluator { 
			move_classes: Vec::<Nag>::new(),
			move_index: Vec::<i32>::new(),
			white_elo: 0,
			black_elo: 0,
			is_blitz: false,
			index: 0 
		}
	}
}

impl Visitor for BoardEvaluator {
	type Result = BoardEvaluator;

	fn begin_game(&mut self) {
        self.move_classes.clear();
		self.move_index.clear();
		self.white_elo = 0;
		self.black_elo = 0;
		self.is_blitz = false;
		self.index = 0;
    }

    fn nag(&mut self, _nag: Nag) {
        self.move_classes.push(_nag);
		self.move_index.push(self.index-1); // called after san(), so index-1
    }

	// for each header
	fn header(&mut self, _key: &[u8], _value: RawHeader<'_>) {
		let header: String = String::from_utf8(_key.to_vec()).unwrap().to_lowercase();
		let value: String = String::from_utf8(_value.as_bytes().to_vec()).unwrap().to_lowercase();
		if header == "event" {
			if value.contains("blitz") {
				self.is_blitz = true;
			}
		}
		else if header == "whiteelo" {
			self.white_elo = value.parse::<i32>().unwrap()
		}
		else if header == "blackelo" {
			self.black_elo = value.parse::<i32>().unwrap()
		}
	}

	// for each move in game
	fn san(&mut self, _san_plus: SanPlus) {
		self.index += 1;
	}

	fn outcome(&mut self, _outcome: Option<Outcome>) {

	}

	// skip game if header contained stuff we don't want
	fn end_headers(&mut self) -> Skip {
		Skip(self.is_blitz)
	}

	// for each comment
	fn comment(&mut self, _comment: RawComment<'_>) {
		let float_eval_regex = Regex::new(r"\[%eval\s(\d+(\.\d+)?)\]").unwrap();
		let mate_eval_regex = Regex::new(r"\[%eval\s#(\d+)\]").unwrap();

		let the_comment: String = String::from_utf8(_comment.as_bytes().to_vec()).unwrap().to_lowercase();
		
		// for a float eval
		if let Some(capture) = float_eval_regex.captures(&the_comment) {
			let value = capture.get(1).unwrap().as_str();
			let number = value.parse::<f32>().unwrap();
        	println!("The eval value is: {}", number);
		}

		// for a mate in x eval
		if let Some(capture) = mate_eval_regex.captures(&the_comment) {
			let value = capture.get(1).unwrap().as_str();
			let number = value.parse::<i32>().unwrap();

			println!("The eval value is: [mate in] {}", number);
		}
	}

    fn begin_variation(&mut self) -> Skip {
        Skip(true) // stay in the mainline
    }

    fn end_game(&mut self) -> Self::Result {
        self.clone()
    }
}

fn main() -> io::Result<()> {

	// let pgn: &str = &fs::read_to_string("../lichess_db_standard_rated_2023-03.pgn").expect("Error reading file");
	let pgn: &str = &fs::read_to_string("Testgame.pgn").expect("Error reading file");

    // let pgn = r#"[Event "Rated Rapid game"]
	// [Site "https://lichess.org/h7SAZCJr"]
	// [Date "2023.03.01"]
	// [Round "-"]
	// [White "ABEDzZ"]
	// [Black "feb1123"]
	// [Result "1-0"]
	// [UTCDate "2023.03.01"]
	// [UTCTime "00:00:15"]
	// [WhiteElo "1267"]
	// [BlackElo "1228"]
	// [WhiteRatingDiff "+9"]
	// [BlackRatingDiff "-17"]
	// [ECO "C26"]
	// [Opening "Vienna Game: Stanley Variation"]
	// [TimeControl "600+0"]
	// [Termination "Normal"]
	
	// 1. e4 { [%eval 0.36] [%clk 0:10:00] } 1... e5 { [%eval 0.32] [%clk 0:10:00] } 2. Nc3 { [%eval 0.1] [%clk 0:09:58] } 2... Nf6 { [%eval 0.17] [%clk 0:09:50] } 3. Bc4 { [%eval 0.0] [%clk 0:09:56] } 3... c6?! { [%eval 0.74] [%clk 0:09:37] } 4. Nf3 { [%eval 0.36] [%clk 0:09:53] } 4... d5 { [%eval 0.23] [%clk 0:0
	// 9:23] } 5. exd5 { [%eval 0.12] [%clk 0:09:50] } 5... cxd5?! { [%eval 0.86] [%clk 0:09:21] } 6. Qe2?? { [%eval -4.07] [%clk 0:09:42] } 6... Nc6?? { [%eval 0.26] [%clk 0:09:04] } 7. Bb5? { [%eval -1.21] [%clk 0:09:37] } 7... Bd7? { [%eval 0.74] [%clk 0:08:58] } 8. Nxe5 { [%eval 0.83] [%clk 0:09:35] } 8... d4?
	// ? { [%eval 6.1] [%clk 0:08:46] } 9. Nxc6+ { [%eval 6.06] [%clk 0:09:31] } 9... Be7 { [%eval 6.18] [%clk 0:08:33] } 10. Nxd8 { [%eval 6.15] [%clk 0:09:15] } 10... Kxd8 { [%eval 7.21] [%clk 0:08:27] } 11. Na4 { [%eval 6.85] [%clk 0:09:04] } 11... Bf5 { [%eval 8.06] [%clk 0:07:47] } 12. O-O { [%eval 8.14] [%cl
	// k 0:08:46] } 12... Bxc2 { [%eval 9.43] [%clk 0:07:32] } 13. b3 { [%eval 8.22] [%clk 0:08:33] } 13... h5 { [%eval 9.98] [%clk 0:07:04] } 14. d3 { [%eval 8.82] [%clk 0:08:17] } 14... a6 { [%eval 11.34] [%clk 0:06:37] } 15. Nb6 { [%eval 9.04] [%clk 0:08:05] } 15... g5? { [%eval #13] [%clk 0:05:56] } 16. Re1?! 
	// { [%eval 14.0] [%clk 0:08:04] } 16... Nd5?! { [%eval #5] [%clk 0:05:53] } 17. Nxd5 { [%eval #4] [%clk 0:08:00] } 17... Bf6 { [%eval #2] [%clk 0:05:47] } 18. Bb2 { [%eval #7] [%clk 0:07:34] } 18... Bxd3 { [%eval #2] [%clk 0:05:38] } 19. Qxd3 { [%eval #4] [%clk 0:07:12] } 19... axb5 { [%eval #3] [%clk 0:05:35
	// ] } 20. Bxd4 { [%eval #5] [%clk 0:07:08] } 20... Bxd4 { [%eval #4] [%clk 0:05:32] } 21. Nb6 { [%eval #5] [%clk 0:07:04] } 21... Ra6 { [%eval #3] [%clk 0:05:17] } 22. Qxd4+ { [%eval #4] [%clk 0:06:54] } 22... Kc7 { [%eval #4] [%clk 0:05:14] } 23. Rac1+ { [%eval #3] [%clk 0:06:48] } 23... Kb8 { [%eval #3] [%c
	// lk 0:05:11] } 24. Qxh8+ { [%eval #3] [%clk 0:06:43] } 24... Ka7 { [%eval #3] [%clk 0:05:09] } 25. Qd4 { [%eval #3] [%clk 0:06:34] } 25... f5 { [%eval #3] [%clk 0:05:03] } 26. Nd7+ { [%eval #2] [%clk 0:06:33] } 26... Ka8 { [%eval #1] [%clk 0:05:01] } 27. Rc8# { [%clk 0:06:33] } 1-0
	// "#;

    let mut reader = BufferedReader::new_cursor(&pgn[..]);

    // let mut counter = MoveCounter::new();
	let mut evaluator = BoardEvaluator::new();
    let moves = reader.read_game(&mut evaluator)?;

	println!("Moves read");

    for nag in moves.iter() {
		println!("nag {:?}", nag)
	}
    Ok(())
}
