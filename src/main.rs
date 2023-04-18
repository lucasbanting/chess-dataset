use pgn_reader::{BufferedReader, Nag, Outcome, RawComment, RawHeader, SanPlus, Skip, Visitor};
use regex::Regex;
use shakmaty::{Chess, Color, Position, Role, Board, Square, CastlingSide, EnPassantMode, Piece};
use std::{fs::File, io};
use std::time::Instant;
use polars_core::prelude::*;
use polars_io::prelude::*;
use lazy_static::lazy_static;

fn sigmoid(val: f32) -> f32 {
	return 1.0 / (1.0 + (-val).exp());
}

#[derive(Clone, Debug)]
struct EncodedBoard {
	board: Vec<i8>
}

fn encode_board(board: &Board, game: &dyn Position) -> EncodedBoard{
	let mut board_array = Vec::<i8>::with_capacity(72);

	for i in 0..64 as u32{
		let mut val: i8 = 0;
		
		if let Some(piece) = board.piece_at(Square::new(i)) {
			val = piece.role as i8;
			if piece.color == Color::Black {
				val = -val;
			}
		}
		
		board_array.push(val);
	}

	// -1 black, +1 white
	let mut turn: i8 = -1;
	if game.turn() == Color::White {
		turn = 1;
	}
	board_array.push(turn);

	// castling
	let castles = game.castles();
	let mut white_king_castle: i8 = 0;
	let mut white_queen_castle: i8 = 0;
	let mut black_king_castle: i8 = 0;
	let mut black_queen_castle: i8 = 0;

	if castles.has(Color::White, CastlingSide::KingSide) {
		white_king_castle = 1;
	}
	if castles.has(Color::White, CastlingSide::QueenSide) {
		white_queen_castle = 1;
	}
	if castles.has(Color::Black, CastlingSide::KingSide) {
		black_king_castle = 1;
	}
	if castles.has(Color::Black, CastlingSide::QueenSide) {
		black_queen_castle = 1;
	}
	board_array.push(white_king_castle);
	board_array.push(white_queen_castle);
	board_array.push(black_king_castle);
	board_array.push(black_queen_castle);

	// ep square
	let mut ep_square: i8 = 0;
	if let Some(square) = game.ep_square(EnPassantMode::Legal) {
		ep_square = square as i8;
	}
	board_array.push(ep_square);

	// half move clock
	board_array.push(game.halfmoves() as i8); // unsafe operation
	board_array.push(0); // ignore fullmoves as we can just calc after
	
	return EncodedBoard{
		board: board_array.clone()
	};
}

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
	win_chances: Vec<f32>,
	move_classes: Vec<u8>,
    move_class_idx: Vec<i32>,
    evals: Vec<f32>,
    evals_idx: Vec<i32>,
    mate_evals: Vec<i32>,
    mate_evals_idx: Vec<i32>,
    white_elo: i32,
    black_elo: i32,
    bitboards: Vec<EncodedBoard>,
}

#[derive(Clone, Debug)]
struct BoardEvaluator {
    move_classes: Vec<Nag>,
    move_class_index: Vec<i32>,
	win_chances: Vec<f32>,
    evals: Vec<f32>,
    evals_idx: Vec<i32>,
    mate_evals: Vec<i32>,
    mate_evals_idx: Vec<i32>,
    white_elo: i32,
    black_elo: i32,
    skip: bool,
    index: i32,
    current_pos: Chess,
    bitboards: Vec<EncodedBoard>,
}

impl BoardEvaluator {
    fn new() -> BoardEvaluator {
        BoardEvaluator {
            move_classes: Vec::<Nag>::new(),
            move_class_index: Vec::<i32>::new(),
			win_chances: Vec::<f32>::new(),
            evals: Vec::<f32>::new(),
            evals_idx: Vec::<i32>::new(),
            mate_evals: Vec::<i32>::new(),
            mate_evals_idx: Vec::<i32>::new(),
            white_elo: 0,
            black_elo: 0,
            skip: false,
            index: 0,
            current_pos: Chess::default(),
            bitboards: Vec::<EncodedBoard>::new(),
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
        self.skip = false;
        self.index = 0;
        self.current_pos = Chess::default();
        self.bitboards.clear();
		self.win_chances.clear()
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
                self.skip = true;
            }
			else if value.contains("bullet") {
				self.skip = true;
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

            // let (role, colour) = self.current_pos.board().to_owned().into_bitboards();

            // let bitboard = BitBoard {
            //     pawn: role.get(Role::Pawn).0,
            //     bishop: role.get(Role::Bishop).0,
            //     knight: role.get(Role::Knight).0,
            //     rook: role.get(Role::Rook).0,
            //     queen: role.get(Role::Queen).0,
            //     king: role.get(Role::King).0,
            //     white: colour.get(Color::White).0,
            //     black: colour.get(Color::Black).0,
            // };

            self.bitboards.push(encode_board(self.current_pos.board(), &self.current_pos));
        }
    }

    fn outcome(&mut self, _outcome: Option<Outcome>) {}

    // skip game if header contained stuff we don't want
    fn end_headers(&mut self) -> Skip {
        Skip(self.skip)
    }

    // for each comment
    fn comment(&mut self, _comment: RawComment<'_>) {
		lazy_static! {
			static ref FLOAT_EVAL_REGEX: Regex = Regex::new(r"\[%eval\s(-?\d+(\.\d+)?)\]").unwrap();
		}

		lazy_static! {
			static ref MATE_EVAL_REGEX: Regex = Regex::new(r"\[%eval\s#(-?\d+)\]").unwrap();
		}
        
        let the_comment: String = String::from_utf8(_comment.as_bytes().to_vec())
            .unwrap()
            .to_lowercase();

        // for a float eval
        if let Some(capture) = FLOAT_EVAL_REGEX.captures(&the_comment) {
            let value = capture.get(1).unwrap().as_str();
            let number = value.parse::<f32>().unwrap();
            // println!("The eval value is: {}", number);

            // self.evals.push(number);
            // self.evals_idx.push(self.index);
			self.win_chances.push(sigmoid(number));
        } 

        // for a mate in x eval
        if let Some(capture) = MATE_EVAL_REGEX.captures(&the_comment) {
            let value = capture.get(1).unwrap().as_str();
            let number = value.parse::<i32>().unwrap();

            // println!("The eval value is: [mate in] {}", number);
            // self.mate_evals.push(number);
            // self.mate_evals_idx.push(self.index);

			let mut num: f32 = 0.0;
			if number > 0 {
				num = 1.0;
			}
			self.win_chances.push(num)
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
			win_chances: self.win_chances.clone(),
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

// fn main() -> io::Result<()>{
// 	let mut file = File::open("test.parquet").expect("Couldn't file file");

// 	let df = ParquetReader::new(&mut file).finish().unwrap();
// 	println!("{:?}", df[0].to_list());

// 	Ok(())
// }

fn main() -> io::Result<()> {
    // let pgn: &str = &fs::read_to_string("../lichess_db_standard_rated_2023-03.pgn")
    //     .expect("Error reading file");
    // let pgn: &str = &fs::read_to_string("Testgame.pgn").expect("Error reading file");
	// let mut reader = BufferedReader::new_cursor(&pgn[..]);

	let file = File::open("../lichess_db_standard_rated_2023-03.pgn.zst").expect("Couldn't file file");

	let uncompressed: Box<dyn io::Read> = Box::new(zstd::Decoder::new(file)?);

    let mut reader = BufferedReader::new(uncompressed);

    // let mut counter = MoveCounter::new();
    let mut evaluator = BoardEvaluator::new();

    let now = Instant::now();
    let mut total_count = 0;
	let mut eval_count = 0;

	let mut games = Vec::<GameInfo>::new();

	while reader.has_more().unwrap() {
        let board = reader.read_game(&mut evaluator)?;

        let unwrapped_board = board.unwrap();

        if unwrapped_board.win_chances.len() > 0 {
			eval_count += 1;

			games.push(unwrapped_board);

			if eval_count % 100 == 0 {
				println!("Total processed: {} Eval count: {}", total_count, eval_count);
			}
        }
        total_count += 1;

		

        if total_count > 10_000_000 {
            break;
        }
    }

	let white_elo = Series::new(
		"white_elo",
		games.iter().map(|g| g.white_elo).collect::<Vec<_>>()
	);

	let black_elo = Series::new(
		"black_elo",
		games.iter().map(|g| g.black_elo).collect::<Vec<_>>()
	);

	let win_chances = Series::new(
		"win_chances",
		games.iter().map(|g| g.win_chances.iter().collect::<Series>()).collect::<Vec<_>>()
	);

	let move_class: Series = Series::new(
        "move_class",
        games.iter().map(|g| g.move_classes.iter().collect::<Series>()).collect::<Vec<_>>()
	);

	let move_class_idx: Series = Series::new(
        "move_class_idx",
        games.iter().map(|g| g.move_class_idx.iter().collect::<Series>()).collect::<Vec<_>>()
	);

	let mut board_lists = Vec::with_capacity(games.len());

	for game in games {
		let mut game_positions = Vec::with_capacity(game.bitboards.len());

		for position in game.bitboards {
			let position_list = Series::new("pos", position.board);

			game_positions.push(position_list);
		}	
		board_lists.push(Series::new("game", game_positions));
	}

	let game_series = Series::new("games", board_lists);

	let mut df = DataFrame::new(
		vec![
				white_elo,
				black_elo,
				win_chances,
				game_series,
				move_class,
				move_class_idx
			]
		).unwrap();
	
	let mut file = std::fs::File::create("../BoardInfoFrameMedium.parquet").unwrap();
	ParquetWriter::new(&mut file).finish(&mut df).unwrap();

	println!("{}", df);

	

	// let evals: Series = Series::new(
    //     "evals",
    //     games.iter().map(|g| g.evals.iter().collect::<Series>()).collect::<Vec<_>>()
	// );

	// let evals_idx: Series = Series::new(
    //     "evals_idx",
    //     games.iter().map(|g| g.evals_idx.iter().collect::<Series>()).collect::<Vec<_>>()
	// );

	// let mate_evals: Series = Series::new(
    //     "mate_evals",
    //     games.iter().map(|g| g.mate_evals.iter().collect::<Series>()).collect::<Vec<_>>()
	// );

	// let mate_evals_idx: Series = Series::new(
    //     "mate_evals_idx",
    //     games.iter().map(|g| g.mate_evals_idx.iter().collect::<Series>()).collect::<Vec<_>>()
	// );

	// let pawns: Series = Series::new(
	// 	"pawns",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.pawn).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let bishops: Series = Series::new(
	// 	"bishops",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.bishop).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let knights: Series = Series::new(
	// 	"knights",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.knight).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let rooks: Series = Series::new(
	// 	"rooks",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.rook).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let queens: Series = Series::new(
	// 	"queens",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.queen).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let kings: Series = Series::new(
	// 	"kings",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.king).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let white_mask: Series = Series::new(
	// 	"white_mask",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.white).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let black_mask: Series = Series::new(
	// 	"black_mask",
	// 	games.iter().map(|g| g.bitboards.iter().map(|b| b.black).collect::<Series>()).collect::<Vec<_>>()
	// );

	// let mut df = DataFrame::new(
	// 	vec![
	// 			white_elo,
	// 			black_elo,
	// 			evals,
	// 			evals_idx,
	// 			mate_evals,
	// 			mate_evals_idx,
	// 			move_class, 
	// 			move_class_idx,
	// 			pawns,
	// 			bishops,
	// 			knights,
	// 			rooks,
	// 			queens,
	// 			kings,
	// 			white_mask,
	// 			black_mask
	// 		]
	// 	).unwrap();
	
	// let mut file = std::fs::File::create("../BoardInfoFrameLarge.parquet").unwrap();
	// ParquetWriter::new(&mut file).finish(&mut df).unwrap();

	// println!("{}", df);

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
