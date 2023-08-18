// (C) Copyright 2023 Ars Militaris Dev

use bevy::{prelude::*, render::camera::ScalingMode, log::LogPlugin, diagnostic::LogDiagnosticsPlugin, diagnostic::FrameTimeDiagnosticsPlugin, window::*};

use bevy::ecs::system::SystemParam;
use bevy::ecs::world::World;

use bevy::utils::Duration;

use gridly_grids::VecGrid;
use gridly::prelude::*;

use std::fs;
use std::collections::HashMap;

use csv::Reader;
use csv::StringRecord;

use kafka::producer::{Producer, Record, RequiredAcks};

use bevy_quinnet::{
    server::{
        certificate::CertificateRetrievalMode, ConnectionLostEvent, Endpoint, QuinnetServerPlugin,
        Server, ServerConfiguration,
    },
    shared::ClientId,
};

use serde::{Deserialize, Serialize};

use rand::Rng;

use pathfinding::prelude::astar;
use std::cell::RefCell;

pub mod kafka_am;

#[derive(Serialize, Deserialize)]
enum ClientMessage {
	GetClientId,
	StartGame,
	LoadingComplete,
	WaitTurnComplete,
	Wait,
	Move {
		origin: Pos,
		destination: Pos,
	},
	BasicAttack {
		attacker: Pos,
		target: Pos,
		damage: usize,
	},
}

#[derive(Serialize, Deserialize)]
enum ServerMessage {
	ClientId {
		client_id: ClientId
	},
	StartGame { 
		client_id: ClientId,
	},
	StartGame2,
	PlayerTurn {
		client_id: ClientId,
		current_unit: usize,
	},
	WaitTurn {
		wait_turns: Vec<(UnitId, WTCurrent)>,
	},
	Wait,
	Move {
		origin: Pos,
		destination: Pos,
	},
	BasicAttack {
		attacker: Pos,
		target: Pos,
		damage: usize,
		is_counterattack: bool,
	},
}

struct PlayerTurnMessage {
	client_id: ClientId,
	current_unit: usize,
}

#[derive(Reflect)]
enum UnitAction {
	Move {
		origin: Pos,
		destination: Pos,
		timer: Timer,
	},
	Talk {
		message: String,
	},
	BasicAttack {
		target: Pos,
		is_counterattack: bool,
		damage: usize,
	},
	DoNothing,
}

impl Default for UnitAction {
    fn default() -> Self {
        UnitAction::DoNothing
    }
}

#[derive(Reflect)]
#[reflect(Default)]
enum Direction {
	East,
	South,
	West,
	North,
}

impl Direction {
	fn from_string(dir_string: String) -> Direction {
		
		match dir_string.as_str() {
			"East" => Direction::East,
			"South" => Direction::South,
			"West" => Direction::West,
			"North" => Direction::North,
			_ => panic!("Invalid Direction string: {}", dir_string),
		}
	}
}

impl Default for Direction {
	fn default() -> Self {
        Direction::East
    }
}



// COMPONENTS

#[derive(Component)]
struct NakedSwordsman {

}

#[derive(Component)]
struct CurrentUnit {

}

#[derive(Component)]
struct Attacker {}

#[derive(Component)]
struct Target {}

#[derive(Component)]
struct MoveTile {}

#[derive(Component)]
struct MoveTiles {
	move_tiles: Vec<Pos>,
}

#[derive(Component)]
struct AttackTile {}

#[derive(Component)]
struct AttackTiles {
	attack_tiles: Vec<Pos>,
}

#[derive(Component)]
struct MoveActions {
	move_actions: Vec<MoveAction>,
}

#[derive(Component)]
struct MoveAction {
	origin: Pos,
	destination: Pos,
	timer: Timer,
}

#[derive(Component)]
struct TalkAction {
	message: String,
}

#[derive(Component)]
struct BasicAttackAction {
	target: Pos,
	is_counterattack: bool,
	damage: usize,
}

#[derive(Component)]
struct DoNothingAction;

#[derive(Component, Reflect, Default)]
struct UnitActions {
	unit_actions: Vec<UnitActionTuple>,
	processing_unit_action: bool,
}

#[derive(Reflect)]
#[reflect(Default)]
struct UnitActionTuple(UnitAction, f32);

impl Default for UnitActionTuple {
    fn default() -> Self {
        UnitActionTuple(UnitAction::DoNothing, 0.0)
    }
}

#[derive(Component)]
struct Cursor {
	x: usize,
	y: usize,
}

#[derive(Component)]
struct Map {
	map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>>,
}

#[derive(Component)]
struct Tile;

#[derive(Component, Debug, Clone, PartialEq)]
enum TileType {
	Grass, 
}

#[derive(Component)]
struct GameText;

#[derive(Component)]
struct Unit;

#[derive(Component, Clone, Reflect, Default, Eq, PartialEq, Hash, Copy, Debug, Serialize, Deserialize)]
#[reflect(Default)]
struct Pos {
	x: usize,
	y: usize,
}

#[derive(Component, Clone, Serialize, Deserialize, Debug)]
struct UnitId { value: usize, }

impl Default for UnitId {
	fn default() -> Self {
        UnitId {
            value: 1,
        }
    }
}

#[derive(Component)]
struct UnitTeam { value: usize, }

#[derive(Component)]
struct UnitName { value: String, }

#[derive(Component)]
struct UnitClass { value: String, }

#[derive(Component)]
struct PosX { value: usize, }

#[derive(Component)]
struct PosY { value: usize, }

#[derive(Component)]
struct WTMax { value: usize, }

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
struct WTCurrent { value: usize, }

#[derive(Component)]
struct HPMax { value: usize, }

#[derive(Component)]
struct HPCurrent { value: usize, }

#[derive(Component)]
struct MPMax { value: usize, }

#[derive(Component)]
struct MPCurrent { value: usize, }

#[derive(Component)]
struct STR { value: usize, }

#[derive(Component)]
struct VIT { value: usize, }

#[derive(Component)]
struct INT { value: usize, }

#[derive(Component)]
struct MEN { value: usize, }

#[derive(Component)]
struct AGI { value: usize, }

#[derive(Component)]
struct DEX { value: usize, }

#[derive(Component)]
struct LUK { value: usize, }

#[derive(Component, Default, Reflect)]
#[reflect(Default)]
struct DIR { direction: Direction, }

#[derive(Component, Default, Reflect)]
#[reflect(Default)]
struct MovementRange { value: isize, }

#[derive(Component, Default, Reflect)]
#[reflect(Default)]
struct AttackRange { value: isize, }

#[derive(Component, Default, Reflect, Clone, Copy)]
#[reflect(Default)]
enum AttackType {
	#[default]
	Melee,
	Ranged,
}

impl AttackType {
	fn from_string(string: String) -> AttackType {
		match string.as_str() {
			"Melee" => { return AttackType::Melee; },
			"Ranged" => { return AttackType::Ranged; },
			_ => { panic!("Invalid AttackType string: {}.", string); },
		}
	}
}

#[derive(Bundle)]
struct UnitAttributes {
	unit_id: UnitId,
	unit_team: UnitTeam,
	unit_name: UnitName,
	unit_class: UnitClass,
	pos_x: PosX,
	pos_y: PosY,
	wt_max: WTMax,
	wt_current: WTCurrent,
	hp_max: HPMax,
	hp_current: HPCurrent,
	mp_max: MPMax,
	mp_current: MPCurrent,
	str: STR,
	vit: VIT,
	int: INT,
	men: MEN,
	agi: AGI,
	dex: DEX,
	luk: LUK,
	dir: DIR,
	movement_range: MovementRange,
	attack_range: AttackRange,
	attack_type: AttackType,
}

// STATES

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
enum GameState {
	#[default]
	MainMenu,
	Loading,
	ClientsLoading,
	LoadMap,
	Battle,
	Move,
	WaitTurn,
}

// EVENTS

#[derive(Event)]
struct GameStartEvent;

#[derive(Event)]
struct MapReadEvent {
	pub map: Vec<Vec<String>>,
}

#[derive(Event)]
struct MapSetupEvent;

#[derive(Event)]
struct UnitsReadEvent {
	pub units: Vec<StringRecord>,
}

#[derive(Event)]
struct UnitsGeneratedEvent;

// RESOURCES

#[derive(Resource)]
struct Game {
	current_unit: usize,
	current_team: usize,
	has_started: bool,
	players: HashMap<usize, ControlledBy>,
	winner: ControlledBy,
}

impl Default for Game {
	fn default() -> Self {
        Game {
            current_unit: 0,
            current_team: 1,
            has_started: false,
            players: HashMap::new(),
            winner: ControlledBy::None, 
        }
    }
}

enum ControlledBy {
	Player,
	AI,
	None,
}

#[derive(Resource, Default)]
struct Timers {
	two_second_timer: Timer,
	six_second_timer: Timer,
}

#[derive(Resource, Default)]
struct PlayerTurnMessages {
	messages: Vec<(PlayerTurnMessage, Timer)>,
}

#[derive(Resource, Default)]
struct PlayerLoadings {
	loadings: HashMap<ClientId, bool>,
}

// Client & Server
fn main() {
	
    App::new()
		.add_plugins(MinimalPlugins)
		.add_plugins(LogPlugin::default())
		.add_plugins(AssetPlugin::default())
		//.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
		.add_plugin(QuinnetServerPlugin::default())
		.add_state::<GameState>()
		.add_event::<GameStartEvent>()
		.add_event::<MapReadEvent>()
		.add_event::<MapSetupEvent>()
		.add_event::<UnitsReadEvent>()
		.add_event::<UnitsGeneratedEvent>()
		.init_resource::<Game>()
		.init_resource::<Timers>()
		.init_resource::<PlayerTurnMessages>()
		.init_resource::<PlayerLoadings>()
		.add_systems(OnEnter(GameState::MainMenu), start_listening)
		.add_systems(Update,
						handle_client_messages
							.run_if(in_state(GameState::MainMenu))
		)
//		.add_systems(Update,
//						(read_map_system, setup_map_system, read_battle_system, generate_units_system, place_units_on_map_system)
//							.run_if(in_state(GameState::Loading))
//		)
		.add_systems(Update, 
						handle_loading_complete_messages
							.run_if(in_state(GameState::ClientsLoading))
		)
		.add_systems(Update, send_player_turn_messages
							.run_if(in_state(GameState::WaitTurn))
		)
		.add_systems(Update, send_player_turn_messages
							.run_if(in_state(GameState::Battle))
		)
		.add_systems(Update, check_loadings.run_if(in_state(GameState::ClientsLoading)))
		.add_systems(OnEnter(GameState::Loading), on_enter_loading_state)
		.add_systems(OnEnter(GameState::Loading), setup_game_resource_system)
		.add_systems(OnEnter(GameState::Loading), setup_grid_system)
		.add_systems(OnEnter(GameState::Loading), (apply_deferred, spawn_units)
			.chain()
			.after(setup_grid_system)
		)
		.add_systems(Update, tick_move_timer
			.run_if(in_state(GameState::Move))
		)
		.add_systems(Update, (apply_deferred, handle_move_state, apply_deferred)
			.chain()
			.run_if(in_state(GameState::Move))
		)
		.add_systems(Update, (process_unit_actions, apply_deferred)
			.chain()
			.run_if(in_state(GameState::Battle))
		)
		.add_systems(Update, (apply_deferred, process_move_actions, apply_deferred)
			.chain()
			.run_if(in_state(GameState::Battle))
		)
//		.add_systems(Update, (apply_deferred, process_talk_actions, apply_deferred)
//			.chain()
//			.run_if(in_state(GameState::Battle))
//		)
		.add_systems(Update, (apply_deferred, process_basic_attack_actions, apply_deferred)
			.chain()
			.run_if(in_state(GameState::Battle))
		)
		.add_systems(Update, handle_unit_death
			.run_if(in_state(GameState::Battle))
		)
		//.add_systems(Update, z_order_system
		//	.run_if(in_state(GameState::LoadMap))
		//	.run_if(text_already_setup)
		//)
		//.add_systems(Update, move_camera_system
		//	.run_if(in_state(GameState::LoadMap))
		//)
		//.add_systems(OnEnter(GameState::LoadMap), (apply_deferred, spawn_gaul_warrior)
		//	.chain()
		//	.after(setup_text_system)
		//)
		//.add_systems(OnEnter(GameState::LoadMap), (apply_deferred, spawn_naked_swordsman)
			//.chain()
			//.after(spawn_gaul_warrior)
			//.after(setup_text_system)
		//)
		//.add_systems(Update, move_gaul_warrior
		//	.run_if(in_state(GameState::LoadMap))
		//	.run_if(warrior_already_spawned)
		//)
		//.add_systems(Update, (process_unit_actions, apply_deferred)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//	.run_if(warrior_already_spawned)
		//)
		//.add_systems(Update, (apply_deferred, first_move_unit_action)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap).and_then(run_once()))
		//	.after(spawn_naked_swordsman)
		//)
		//.add_systems(Update, (apply_deferred, process_move_actions, apply_deferred)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//)
		//.add_systems(Update, (apply_deferred, process_talk_actions, apply_deferred)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//)
		//.add_systems(Update, setup_two_seconds_timer
		//	.run_if(in_state(GameState::LoadMap).and_then(run_once()))
		//	//.after(first_move_unit_action)
		//)
		//.add_systems(OnEnter(GameState::LoadMap), (apply_deferred, cutscene_1)
		//	.chain()
		//	.after(spawn_naked_swordsman)
		//)
		//.add_systems(Update, second_move_action
		//	.run_if(in_state(GameState::LoadMap))
		//	.run_if(two_seconds_have_passed)
		//	.after(first_move_unit_action)
		//)
		//.add_systems(Update, (apply_deferred, first_talk_unit_action)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap).and_then(run_once()))
		//	//.after(third_move_action)
		//)
		//.add_systems(Update, third_move_action
		//	.run_if(six_seconds_have_passed)
		//	.after(second_move_action)
		//)
		//.add_systems(Update, (second_move_action.run_if(two_seconds_have_passed), third_move_action.run_if(six_seconds_have_passed), first_talk_unit_action.run_if(run_once()))
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//	.after(first_move_unit_action)
		//)
		//.add_systems(Update, tick_timers
		//	.run_if(in_state(GameState::LoadMap))
		//)
		//.add_systems(Update, (apply_deferred, test_system_3)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//)
		//.add_systems(Update, print_gaul_warrior
		//	.run_if(in_state(GameState::LoadMap))
		//	.run_if(warrior_already_spawned)
		//)
		//.add_systems(Update, (apply_deferred, center_camera_on_unit)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//	.run_if(warrior_already_spawned)
		//)
		//.add_systems(Update, (apply_deferred, test_ortho_projection)
		//	.chain()
		//	.run_if(in_state(GameState::LoadMap))
		//)
		.add_systems(Update,
						handle_wait_turn_completed
							.run_if(in_state(GameState::Battle))
		)
		.add_systems(Update,
						(wait_turn_system, handle_wait_turn_completed)
							.run_if(in_state(GameState::WaitTurn))
		)
		.add_systems(OnExit(GameState::WaitTurn), on_complete_wait_turn)
		.run();
}

// SYSTEMS

//// Server
//fn read_map_system(mut events: EventReader<GameStartEvent>, mut events2: EventWriter<MapReadEvent>) {
//	
//	for event in events.iter() {
//		//info!("DEBUG: Reading map file...");
//	
//		// This function has a bug when using the '\\' as the path separator.
//		// It won't work on Linux.
//		let file_contents = fs::read_to_string("src/map.txt").unwrap();
//		
//		//info!("DEBUG: Read map file.");
//		
//		// Separate map into lines.
//		let map_lines: Vec<&str> = file_contents.split('\n').collect();
//		//info!("DEBUG: Map line 1 is: \n{}", map_lines[0]);
//		
//		// Separate lines and build 2D-array.
//		info!("DEBUG: Starting to build 2D array of map...");
//		let mut map: Vec<Vec<String>> = Vec::new();
//		for i in 0..map_lines.len() {
//			let mut map_line: Vec<String> = Vec::new();
//			let line = map_lines[i];
//			let line_splitted: Vec<&str> = line.split(' ').collect();
//			for j in 0..line_splitted.len() {
//				let map_cell = line_splitted[j].to_owned();
//				map_line.push(map_cell);
//			}
//			map.push(map_line);
//		}
//		info!("DEBUG: Finished building 2D array of map.");
//		
//		//info!("DEBUG: Printing map file...");
//		//info!("{}", file_contents);
//		//info!("DEBUG: Printed map file...");
//		
//		events2.send(MapReadEvent {
//						map: map,
//					});
//	}
//}
//
//// Client & Server
//fn setup_map_system(mut events: EventReader<MapReadEvent>, mut events2: EventWriter<MapSetupEvent>, mut commands: Commands, asset_server: Res<AssetServer>) {
//	
//	for event in events.iter() {
//		
//		info!("DEBUG: Starting to set up map in the ECS World...");
//		info!("DEBUG: Finished setting up map in the ECS World.");
//		events2.send(MapSetupEvent);
//	}
//}
//
//// Server
//fn read_battle_system(mut events: EventReader<MapSetupEvent>, mut events2: EventWriter<UnitsReadEvent>) {
//	for event in events.iter() {
//		// This function has a bug when using the '\\' as the path separator.
//		// It won't work on Linux.
//		let mut rdr = Reader::from_path("src/the_patrol_ambush_data.csv").unwrap();
//		let mut records: Vec<StringRecord> = Vec::new();
//		for result in rdr.records(){
//			let record = result.unwrap();
//			//info!("{:?}", record);
//			records.push(record);
//		}
//		events2.send(UnitsReadEvent {
//							units: records,
//						});
//	}
//}
//
//// Server
//fn generate_units_system(mut events: EventReader<UnitsReadEvent>, mut events2: EventWriter<UnitsGeneratedEvent>, mut commands: Commands) {
//	
//	for event in events.iter() {
//		// For each record, create an Entity for an unit.
//		let records = &event.units;
//		for record in records {
//			info!("DEBUG: Creating new unit...");
//			commands.spawn((
//				UnitAttributes {
//					unit_id : UnitId { value: record[0].parse().unwrap(), },
//					unit_team : UnitTeam { value: record[1].parse().unwrap(), },
//					unit_name : UnitName { value: record[2].to_string(), },
//					unit_class : UnitClass { value: record[3].to_string(), },
//					pos_x : PosX { value: record[4].parse().unwrap(), }, 
//					pos_y : PosY { value: record[5].parse().unwrap(), },
//					wt_max : WTMax { value: record[6].parse().unwrap(), },
//					wt_current : WTCurrent{ value: record[7].parse().unwrap(), },
//					hp_max : HPMax { value: record[8].parse().unwrap(), },
//					hp_current : HPCurrent { value: record[9].parse().unwrap(), },
//					mp_max : MPMax { value: record[10].parse().unwrap(), },
//					mp_current : MPCurrent { value: record[11].parse().unwrap(), },
//					str : STR { value: record[12].parse().unwrap(), },
//					vit : VIT { value: record[13].parse().unwrap(), },
//					int : INT { value: record[14].parse().unwrap(), },
//					men : MEN { value: record[15].parse().unwrap(), },
//					agi : AGI { value: record[16].parse().unwrap(), },
//					dex : DEX { value: record[17].parse().unwrap(), },
//					luk : LUK { value: record[18].parse().unwrap(), },
//				},
//				Unit,
//			));
//		}
//		events2.send(UnitsGeneratedEvent);
//	}
//}
//
//// Server
//fn place_units_on_map_system(mut events: EventReader<UnitsGeneratedEvent>, unit_positions: Query<(&UnitId, &PosX, &PosY)>, mut tiles: Query<(&Tile, &Pos, &mut Text)>, mut commands: Commands, mut next_state: ResMut<NextState<GameState>>) {
//	
//	for event in events.iter() {
//		info!("DEBUG: Starting to place units on map...");
//		info!("DEBUG: Finished placing units on map.");
//
//		info!("DEBUG: Setting GameState to WaitTurn...");
//		//info!("DEBUG: Setting GameState to ClientsLoading...");
//		//commands.insert_resource(NextState(GameState::WaitTurn));	
//		//next_state.set(GameState::ClientsLoading);
//		//info!("DEBUG: Set GameState to ClientsLoading.");
//		next_state.set(GameState::WaitTurn);
//		info!("DEBUG: Set GameState to WaitTurn.");
//	}
//}

// Server
fn wait_turn_system(
mut units: Query<(Entity, &mut WTCurrent, &WTMax, &UnitId, &UnitTeam)>,
mut game: ResMut<Game>,
mut commands: Commands,
mut server: ResMut<Server>,
mut next_state: ResMut<NextState<GameState>>,
mut player_turn_messages: ResMut<PlayerTurnMessages>,
) {
	
	let endpoint = server.endpoint_mut();
	
	// Decrease all units WT. If WT equals 0, set the unit as the current unit turn.
	for (entity, mut wt_current, wt_max, unit_id, unit_team) in units.iter_mut() {
		if wt_current.value == 0 {
			info!("DEBUG: It is now unit {} turn.", unit_id.value);
			game.current_unit = unit_id.value;
			
			//// Send PlayerTurn message.
			//info!("DEBUG: Sending Player Turn message...");
			//endpoint.broadcast_message(ServerMessage::PlayerTurn { client_id: unit_team.value as u64, current_unit: unit_id.value, }).unwrap();
			//info!("DEBUG: Sent Player Turn message.");
			
			// Schedule a PlayerTurn message for 0.5 seconds from now.
			// This is a fix for BUG#6
			player_turn_messages.messages.push((PlayerTurnMessage { client_id: unit_team.value as u64, current_unit: unit_id.value, }, Timer::from_seconds(0.5, TimerMode::Once)));
			
			// Assign the `CurrentUnit` component to the current unit.
			commands.entity(entity).insert(CurrentUnit {});
			
			info!("DEBUG: Setting GameState to Battle..."); 
			//commands.insert_resource(NextState(GameState::Battle));
			next_state.set(GameState::Battle);
			info!("DEBUG: Set GameState to Battle.");
		} else {
			wt_current.value = wt_current.value - 1;
			info!("DEBUG: Decreased unit {:?} WT. It is now {:?}.", unit_id.value, wt_current.value);
		}
	}
}

// Server
fn on_complete_wait_turn(mut server: ResMut<Server>, units: Query<(&UnitId, &WTCurrent)>) {
	
	// Build WaitTurn message.
	let mut unit_wts: Vec<(UnitId, WTCurrent)> = Vec::new();
	for (unit_id, current_wt) in units.iter() {
		unit_wts.push((unit_id.clone(), current_wt.clone()));
	}
	// Send WaitTurn message.
	let endpoint = server.endpoint_mut();
	info!("DEBUG: Sending WaitTurn message...");
	endpoint.broadcast_message(ServerMessage::WaitTurn {
		wait_turns: unit_wts,
	}).unwrap();
	info!("DEBUG: Sent WaitTurn message.");
}

// Server
fn handle_wait_turn_completed (
mut server: ResMut<Server>,
mut commands: Commands,
mut map_query: Query<&mut Map>,
mut current_unit_query: Query<(Entity, &mut UnitActions, &mut WTCurrent, &WTMax), With<CurrentUnit>>,
mut next_state: ResMut<NextState<GameState>>,
) {
	let mut endpoint = server.endpoint_mut();

	let mut map = &mut map_query.single_mut().map;

	for client_id in endpoint.clients() {
		while let Ok(Some(message)) = endpoint.receive_message_from::<ClientMessage>(client_id) {
			match message {
				ClientMessage::Wait => {
					info!("DEBUG: Received Wait message.");
					
					
//					// Reset the current unit's WT.
//					for (mut wt_current, wt_max, unit_team) in units.iter_mut() {
//						if wt_current.value == 0 {
//							wt_current.value = wt_max.value;
//							info!("DEBUG: Reseted Current Unit's WT. It is now: {:?}.", wt_current);
//							break;
//						}
//					}
					
					// Reset the current unit's WT.
					let (entity, mut unit_actions, mut wt_current, wt_max) = current_unit_query.single_mut();
					wt_current.value = wt_max.value;
					info!("DEBUG: Reseted Current Unit's WT. It is now: {:?}.", wt_current);
					
					// Remove the `CurrentUnit` component from current unit.
					commands.entity(entity).remove::<CurrentUnit>();
					
					// Send Wait message.
					info!("DEBUG: Sending Wait message...");
					endpoint.broadcast_message(ServerMessage::Wait).unwrap();
					info!("DEBUG: Sent Wait message.");
					
					info!("DEBUG: Setting GameState to WaitTurn...");
					//commands.insert_resource(NextState(GameState::WaitTurn));
					next_state.set(GameState::WaitTurn);
					info!("DEBUG: Set GameState to WaitTurn...");
				},
				ClientMessage::Move { origin, destination } => {
					info!("DEBUG: Received Move message from client {}.", client_id);
					
					// Insert `Move` `UnitAction` in the unit.
					let (entity, mut unit_actions, mut wt_current, wt_max) = current_unit_query.single_mut();
					unit_actions.unit_actions.push(UnitActionTuple(UnitAction::Move {
						origin: Pos { x: origin.x, y: origin.y, },
						destination: Pos { x: destination.x, y: destination.y },
						timer: Timer::from_seconds(4.0, TimerMode::Once),
					}, 0.0));
					
					// Send `Move` message to clients.
					endpoint.broadcast_message(ServerMessage::Move {
						origin: Pos { x: origin.x, y: origin.y, },
						destination: Pos { x: destination.x, y: destination.y },
					}).unwrap();
					//info!("DEBUG: Sent `Move` message to clients.");
				},
				ClientMessage::BasicAttack { attacker, target, damage } => {
					info!("DEBUG: Received BasicAttack message from client {}.", client_id);
					
					// Insert `BasicAttack` `UnitAction` in the unit.
					let (entity, mut unit_actions, mut wt_current, wt_max) = current_unit_query.single_mut();
					unit_actions.unit_actions.push(UnitActionTuple(UnitAction::BasicAttack {
						target: Pos { x: target.x, y: target.y, },
						is_counterattack: false,
						damage: damage,
					}, 0.0));
					
					// Insert an `Attacker` marker component on the attacking unit.
					commands.entity(entity).insert(Attacker {});
					
					// Insert the `Target` marker component on the target unit.
					let target_entity = map[target.x][target.y].2[0];
					commands.entity(target_entity).insert(Target {});
				}
				_ => { empty_system(); },
			}
		}
	}
}

// Server
fn setup_game_resource_system(mut commands: Commands) {
	let mut players = HashMap::new();
	players.insert(1, ControlledBy::Player);
	players.insert(2, ControlledBy::Player);
	
	commands.insert_resource(Game {
		current_unit: 0,
		current_team: 1,
		has_started: true,
		players: players,
		winner: ControlledBy::None,
	});
}

// Server
fn start_listening(mut server: ResMut<Server>) {
	server
		.start_endpoint(
			//ServerConfiguration::from_string("127.0.0.1:6000").unwrap(),
			ServerConfiguration::from_string("139.162.244.70:6000").unwrap(),
			CertificateRetrievalMode::GenerateSelfSigned {
				server_hostname: "amserver".to_string(),
			},
		)
		.unwrap();
}

// Server
fn handle_client_messages(
    mut server: ResMut<Server>,
    mut events: EventWriter<GameStartEvent>,
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut units: Query<(&UnitId, &WTCurrent)>,
    mut next_state: ResMut<NextState<GameState>>,
    mut player_loadings: ResMut<PlayerLoadings>,
) {
    let mut endpoint = server.endpoint_mut();
    
    for client_id in endpoint.clients() {
		while let Ok(Some(message)) = endpoint.receive_message_from::<ClientMessage>(client_id) {
			match message {
				// Match on your own message types ...
				ClientMessage::StartGame => {
					// Broadcast StartGame message.
					info!("DEBUG: Sending StartGame message...");
					endpoint.broadcast_message(ServerMessage::StartGame { client_id: client_id, }).unwrap();
					info!("DEBUG: Sent StartGame message.");
					
					// If the server game hasn't started yet, start the game on the server.
					if game.has_started == false {
						info!("DEBUG: Starting game on server...");
						game.has_started = true;
						events.send(GameStartEvent);
						//info!("DEBUG: Setting GameState to Loading...");
						info!("DEBUG: Setting GameState to ClientsLoading...");
						//commands.insert_resource(NextState(GameState::Loading));
						//next_state.set(GameState::Loading);
						next_state.set(GameState::ClientsLoading);
						info!("DEBUG: Set GameState to ClientsLoading.");
					} else {
						break;
						// Send WaitTurn and PlayerTurn message to new client.
						
						// Build WaitTurn message.
						let mut wts: Vec<(UnitId, WTCurrent)> = Vec::new();
						for (unit_id, wt_current) in units.iter() {
							wts.push((unit_id.clone(), wt_current.clone()));
						}
						info!("DEBUG: Sending WaitTurn message...");
						endpoint.send_message(client_id, ServerMessage::WaitTurn {
							wait_turns: wts,
						}).unwrap();
						info!("DEBUG: Sent WaitTurn message.");
						//info!("DEBUG: Sending PlayerTurn message...");
						//endpoint.send_message(client_id, ServerMessage::PlayerTurn {
							//client_id: client_id,
						//}).unwrap();
						//info!("DEBUG: Sent PlayerTurn message.");
					}               
				},
				ClientMessage::GetClientId => {
					// Register client and its load status.
					player_loadings.loadings.insert(client_id, false);
					
					info!("DEBUG: Sending ClientId message...");
					endpoint.send_message(client_id, ServerMessage::ClientId {
						client_id: client_id,
					}).unwrap();
					info!("DEBUG: Sent ClientId message.");
				},
				_ => { empty_system(); },
			}
		}
    }
}

// Server
fn handle_wait_client_message(mut server: ResMut<Server>, mut commands: Commands, mut next_state: ResMut<NextState<GameState>>) {
	let mut endpoint = server.endpoint_mut();
	
	for client_id in endpoint.clients() {
		while let Ok(Some(message)) = endpoint.receive_message_from::<ClientMessage>(client_id) {
			match message {
				ClientMessage::Wait => {
					info!("DEBUG: Received Wait message.");
					info!("DEBUG: Sending Wait message...");
					endpoint.broadcast_message(ServerMessage::Wait).unwrap();
					info!("DEBUG: Sent Wait message.");
					
					//commands.insert_resource(NextState(GameState::WaitTurn));
					next_state.set(GameState::WaitTurn);
				},
				_ => { 
					info!("DEBUG: Received other message.");
				},
			}
		}
	}
}

// Client & Server
fn setup_grid_system(mut commands: Commands) {
	// Create map.
	info!("DEBUG: Creating map...");
	let mut map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>> = Vec::new();
	for i in 0..30 {
		let mut map_line: Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)> = Vec::new();
		for j in 0..30 {
			map_line.push((1, TileType::Grass, Vec::new(), Vec::new()));
		}
		map.push(map_line);
	}
	
	//for i in 0..3 {
	//	map[i][0].0 = 10;
	//}
	
	info!("DEBUG: Created map.");
	
	commands.spawn((
		Map { map: map },
	));
}

// Client & Server
fn grid_already_setup(query: Query<&Map>) -> bool {
	if query.iter().len() == 0 {
		return false;
	} else {
		return true;
	}
}

// Test
fn test_system_2(
map_query: Query<&Map>,
mut transform_query: Query<&Transform>,
) {
	let map = &map_query.single().map;
	
	let entity_id = map[1][0].3[0];
	
	if let Ok(transform) = transform_query.get(entity_id) {
		info!("DEBUG: Tile at position (1, 0) is at coordinates: {:?}.", transform.translation);
	}
}

// Test
fn test_system_3(
camera_transform_query: Query<&Transform, With<Camera>>,
) {
	let camera_transform = camera_transform_query.single();
	
	info!("DEBUG: Camera is at position: {:?}.", camera_transform.translation);
}

// Server
fn handle_loading_complete_messages(
mut server: ResMut<Server>,
mut next_state: ResMut<NextState<GameState>>,
mut player_loadings: ResMut<PlayerLoadings>,
) {
	let endpoint = server.endpoint_mut();
	
	for client_id in endpoint.clients() {
		while let Ok(Some(message)) = endpoint.receive_message_from::<ClientMessage>(client_id) {
			match message {
				ClientMessage::LoadingComplete => {
					info!("DEBUG: Received LoadingComplete message from client {}.", client_id);
					// Record that the player has loaded.
					player_loadings.loadings.insert(client_id, true);
					
					//endpoint.broadcast_message(ServerMessage::StartGame2).unwrap();
					//info!("DEBUG: Sent StartGame2 message to clients.");
					
					//// Set GameState to Loading.
					//info!("DEBUG: Setting GameState to Loading...");
					//next_state.set(GameState::Loading);
					//info!("DEBUG: Set GameState to Loading.");
				},
				_ => { empty_system() }
			}
		}	
	}
}

// Server
fn check_loadings(
player_loadings: Res<PlayerLoadings>,
mut server: ResMut<Server>,
mut next_state: ResMut<NextState<GameState>>,
) {
	// Compute if all bools in the HashMap are set to true.
	let all_clients_loaded = &player_loadings.loadings.values().all(|&value| value);
	
	if *all_clients_loaded {
		// All clients loaded.
		info!("DEBUG: All clients have loaded.");
		let endpoint = server.endpoint_mut();
		endpoint.broadcast_message(ServerMessage::StartGame2).unwrap();
		info!("DEBUG: Sent StartGame2 message to clients.");
		
		// Set GameState to Loading.
		info!("DEBUG: Setting GameState to Loading...");
		next_state.set(GameState::Loading);
		info!("DEBUG: Set GameState to Loading.");
	}
}

// Server
fn on_enter_loading_state(
mut events: EventWriter<GameStartEvent>,
) {
	info!("DEBUG: Sending GameStartEvent...");
	events.send(GameStartEvent);
	info!("DEBUG: Sent GameStartEvent.");
}

// Server
fn send_player_turn_messages(mut server: ResMut<Server>, mut player_turn_messages: ResMut<PlayerTurnMessages>, time: Res<Time>) {

	let mut messages = &mut player_turn_messages.messages;
	if messages.len() == 0 {
		
	} else if messages[0].1.tick(time.delta()).just_finished() {
		let endpoint = server.endpoint_mut();
		
		// Send PlayerTurn message.
		info!("DEBUG: Sending Player Turn message...");
		endpoint.broadcast_message(ServerMessage::PlayerTurn { client_id: messages[0].0.client_id, current_unit: messages[0].0.current_unit, }).unwrap();
		info!("DEBUG: Sent Player Turn message.");
		
		messages.remove(0);
	}
	
	
}

// Prototype
fn process_unit_actions(
mut commands: Commands,
mut unit_actions_query: Query<(Entity, &mut UnitActions)>,
mut map_query: Query<&mut Map>,
time: Res<Time>,
) {
	let map = &map_query.single_mut().map;
	
	//info!("DEBUG: unit_actions_query length is: {}.", unit_actions_query.iter().len());
	
	for (entity, mut unit_actions) in unit_actions_query.iter_mut() {
		
		if unit_actions.unit_actions.len() == 0 {
			continue;
		} else if unit_actions.processing_unit_action == true {
			continue;
		} else if time.elapsed_seconds() < unit_actions.unit_actions[0].1 {
			continue;
		} else if unit_actions.unit_actions[0].1 == 0.0 || time.elapsed_seconds() >= unit_actions.unit_actions[0].1 {
			unit_actions.processing_unit_action = true;
			
			let current_unit_action = &unit_actions.unit_actions[0].0;
		
			match current_unit_action {
				UnitAction::Move { origin, destination, timer, } => {
					info!("DEBUG: Current unit action is Move.");
					commands.entity(entity).insert(MoveAction { 
						origin: origin.clone(),
						destination: destination.clone(),
						timer: timer.clone(),
					});
				},
				UnitAction::Talk { message } => {
					info!("DEBUG: Current unit action is Talk.");
					commands.entity(entity).insert(TalkAction { message: message.clone(), });
				},
				UnitAction::BasicAttack { target, is_counterattack, damage} => {
					info!("DEBUG: Current unit action is BasicAttack.");
					commands.entity(entity).insert(BasicAttackAction { target: target.clone(), is_counterattack: is_counterattack.clone(), damage: damage.clone(), });
				}
				UnitAction::DoNothing => {
					info!("DEBUG: Current unit action is DoNothing.");
					commands.entity(entity).insert(DoNothingAction);
				}
			}
		}
	}
}

// Prototype
fn process_move_actions(
mut commands: Commands,
mut map_query: Query<&mut Map>,
mut unit_query: Query<(Entity, &mut UnitActions, &mut Pos, &MoveAction, &mut MoveActions)>,
mut next_state: ResMut<NextState<GameState>>,
) {
	let map = &mut map_query.single_mut().map;
	
	for (entity, mut unit_actions, mut pos, move_action, mut move_actions) in unit_query.iter_mut() {
		info!("DEBUG: Processing MoveAction...");
		info!("DEBUG: Move destination is: {}, {}.", move_action.destination.x, move_action.destination.y);
		
		// Calculate path.
		let path = find_path(map.to_vec(), move_action.origin, move_action.destination);
		if let Some(mut path) = path {
			
			let origin_backup: Pos = path[0];
			
			// Remove the first entry because it is the tile the unit is on.
			path.remove(0);
			
			for i in 0..path.len() {
				info!("DEBUG: Path step is {}, {}.", path[i].x, path[i].y);
				// Create MoveAction
				if i == 0 {
					let new_move_action = MoveAction { origin: origin_backup, destination: path[i], timer: Timer::from_seconds(2.0, TimerMode::Once)};
					// Add MoveAction to MoveActions component.
					move_actions.move_actions.push(new_move_action);
				} else {
					let new_move_action = MoveAction { origin: path[i - 1], destination: path[i], timer: Timer::from_seconds(2.0, TimerMode::Once)};
					// Add MoveAction to MoveActions component.
					move_actions.move_actions.push(new_move_action);
				}
			}
		}
		
		// Set GameState to Move
		info!("DEBUG: Setting GameState to Move...");
		next_state.set(GameState::Move);
		info!("DEBUG: Set GameState to Move.");
		
		break;
	}
}

// Prototype
fn handle_move_state(
mut commands: Commands,
mut map_query: Query<&mut Map>,
mut unit_query: Query<(Entity, &mut UnitActions, &mut Pos, &mut MoveActions, &mut DIR), (With<MoveAction>, Without<GameText>)>,
tile_transform_query: Query<&Transform, (With<GameText>, Without<Unit>)>,
mut next_state: ResMut<NextState<GameState>>,
game: Res<Game>,
time: Res<Time>,
) {
	let mut map_component = map_query.single_mut();
	let mut map = &mut map_component.map;

	if unit_query.iter_mut().len() == 0 {
		
		info!("DEBUG: No MoveActions remaining. Setting GameState to Battle.");
		next_state.set(GameState::Battle);
		info!("DEBUG: No MoveActions remaining. Set GameState to Battle.");
		
	} else {
	
		for (entity, mut unit_actions, mut pos, mut move_actions, mut dir) in unit_query.iter_mut() {
			
			if move_actions.move_actions.len() == 0 {
				// This unit has completed its movement.
				info!("DEBUG: Current unit has finished its movement.");
				unit_actions.processing_unit_action = false;
				unit_actions.unit_actions.remove(0);
				commands.entity(entity).remove::<MoveAction>();
				info!("DEBUG: Processed MoveAction.");
			} else {
				let move_action = &move_actions.move_actions[0];
				
				if map[move_action.destination.x][move_action.destination.y].2.len() > 0 {
					info!("DEBUG: Couldn't move unit. There's an unit already there.");
					unit_actions.processing_unit_action = false;
					unit_actions.unit_actions.remove(0);
					commands.entity(entity).remove::<MoveAction>();
					move_actions.move_actions.truncate(0);
					info!("DEBUG: Processed MoveAction.");
				} else {
					// Complete processing of MoveAction.
						
					info!("DEBUG: Completing processing of MoveAction...");
					map[move_action.destination.x][move_action.destination.y].2.push(entity);
					map[pos.x][pos.y].2.pop();
					
					pos.x = move_action.destination.x;
					pos.y = move_action.destination.y;
					
					
					
					move_actions.move_actions.remove(0);
					info!("DEBUG: Processed MoveAction.");
				
				}
			}
		}
	}
}

// Prototype
fn tick_move_timer(
mut move_actions_query: Query<&mut MoveActions, With<MoveAction>>,
time: Res<Time>
) {
	for (mut move_actions) in move_actions_query.iter_mut() {
		if move_actions.move_actions.len() != 0 {
			move_actions.move_actions[0].timer.tick(time.delta());
		}
	}
}

// Prototype
fn process_do_nothing_actions(mut commands: Commands, mut unit_query: Query<(Entity, &mut UnitActions, &DoNothingAction)>) {
	for (entity, mut unit_actions, do_nothing_action) in unit_query.iter_mut() {
		info!("DEBUG: Processing DoNothing action...");
		
		unit_actions.unit_actions.remove(0);
		unit_actions.processing_unit_action = false;
		commands.entity(entity).remove::<DoNothingAction>();
		
		info!("DEBUG: Processed DoNothing action.");
	}	
}

// Prototype
fn process_basic_attack_actions(
mut commands: Commands,
map_query: Query<&Map>,
mut attack_unit_query: Query<(Entity, &UnitId, &mut UnitActions, &STR, &Pos, &mut DIR, &BasicAttackAction), (With<Attacker>, Without<Target>)>,
mut target_unit_query: Query<(&UnitId, &mut UnitActions, &Pos, &mut HPCurrent, &AttackRange, &AttackType), (With<Target>, Without<Attacker>)>,
mut server: ResMut<Server>,
time: Res<Time>,
) {
	let mut endpoint = server.endpoint_mut();

	let map = &map_query.single().map;

	for (entity, unit_id, mut unit_actions, str, pos, mut dir, basic_attack_action) in attack_unit_query.iter_mut() {
		info!("DEBUG: Processing BasicAttack action...");
		
		// Get target entity from map.
		let target_entity = map[basic_attack_action.target.x][basic_attack_action.target.y].2[0];
		
		// Get target health.
		if let Ok((target_id, mut target_unit_actions, target_pos, mut hp_current, attack_range, attack_type)) = target_unit_query.get_mut(target_entity) {
			// Change attacker's direction to face the target.
			// Set the unit's direction.
			if target_pos.x < pos.x {
				dir.direction = Direction::West;
			} else if target_pos.x > pos.x {
				dir.direction = Direction::East;
			}
			if target_pos.y < pos.y {
				dir.direction = Direction::South;
			} else if target_pos.y > pos.y {
				dir.direction = Direction::North;
			}
			
			// Compute a random number between -3 to 3.
			let mut rng = rand::thread_rng();
			let random_dmg = rng.gen_range(0..7);
			let random_dmg_modifier = random_dmg - 3;
			
			
			// Add random modifier to damage.
			// Damage is (STR / 3) + modifier.
			let attack_damage = (str.value / 3) + random_dmg_modifier;
			
			// Send `BasicAttack` message to clients.
			info!("DEBUG: Sending `BasicAttack` message to clients.");
			match unit_actions.unit_actions[0].0 {
				UnitAction::BasicAttack { target, is_counterattack, damage } => {
					endpoint.broadcast_message(ServerMessage::BasicAttack {
						attacker: Pos { x: pos.x, y: pos.y, },
						target: Pos { x: target_pos.x, y: target_pos.y, },
						damage: attack_damage.clone(),
						is_counterattack: is_counterattack,
					}).unwrap();
					info!("DEBUG: Sent `BasicAttack` message to clients.");
				},
				_ => { empty_system(); },
			}
			
			// Subtract damage from target HP.
			if attack_damage > hp_current.value {
				hp_current.value = 0;
				
				// Remove Attacker marker component.
				commands.entity(entity).remove::<Attacker>();
			
				// Remove BasicAttack UnitAction.
				unit_actions.unit_actions.remove(0);
				unit_actions.processing_unit_action = false;
				commands.entity(entity).remove::<BasicAttackAction>();
				
				info!("DEBUG: Processed BasicAttack action.");
				
				return;
			} else {
				hp_current.value -= attack_damage;
			}
			
			info!("DEBUG: Unit {:?} did {:?} damage to unit {:?}.", unit_id, attack_damage, target_id);
			info!("DEBUG: Unit {} now has {} HP.", target_id.value, hp_current.value);
			
			// Remove Target marker component from the target.
			commands.entity(target_entity).remove::<Target>();
			
			match unit_actions.unit_actions[0].0 {
				UnitAction::BasicAttack { target, is_counterattack, damage } => {
					// If it is not already a counter-attack...
					if !is_counterattack {
						// If target is not a ranged unit...
						// Insert a counter-attack.
						match attack_type {
							AttackType::Ranged => { 
								info!("DEBUG: Target is a ranged unit. Won't make a counter-attack.");
							},
							AttackType::Melee => {
								info!("DEBUG: Target is a melee unit. Will make a counter-attack if at range.");
								let target_possible_attacks = find_possible_attacks(map.to_vec(), *target_pos, attack_range.value, *attack_type);
								
								if target_possible_attacks.contains(pos) {
									// Insert a BasicAttack as a counter-attack.
									
									// Schedule a `BasicAttack` `ServerMessage` to
									// 2 seconds from now, so that the client is
									// able to process the first `BasicAttack`.
									target_unit_actions.unit_actions.push(UnitActionTuple(UnitAction::BasicAttack {
										target: Pos { x: pos.x, y: pos.y, },
										is_counterattack: true,
										damage: 0,
									}, (time.elapsed() + Duration::from_secs(2)).as_secs() as f32));
									
									// Insert the Attacker marker component on the counter-attacking unit.
									commands.entity(target_entity).insert(Attacker {});
									
									// Insert the Target marker component on the unit that did the initial attack.
									// This will result in an infinite Attack and CounterAttack loop for now.
									commands.entity(entity).insert(Target {});
									
								}
							}
						}
					}
				},
				_ => { empty_system() },
			}
			
		}
		
		// Remove Attacker marker component.
		commands.entity(entity).remove::<Attacker>();
	
		// Remove BasicAttack UnitAction.
		unit_actions.unit_actions.remove(0);
		unit_actions.processing_unit_action = false;
		commands.entity(entity).remove::<BasicAttackAction>();
		
		info!("DEBUG: Processed BasicAttack action.");
	}	
}

// Client
fn spawn_units(
mut commands: Commands,
asset_server: Res<AssetServer>,
mut map_query: Query<&mut Map>,
tile_transform_query: Query<&Transform, With<GameText>>,
mut next_state: ResMut<NextState<GameState>>,
) {
	info!("DEBUG: Starting to spawn units...");

	let mut map = &mut map_query.single_mut().map;

	let mut rdr = Reader::from_path("src/the_patrol_ambush_data.csv").unwrap();
	let mut records: Vec<StringRecord> = Vec::new();
	for result in rdr.records(){
		let record = result.unwrap();
		//info!("{:?}", record);
		records.push(record);
	}
	
	for record in records {
		info!("DEBUG: Creating new unit...");
		let entity_id = commands.spawn((
			UnitAttributes {
				unit_id : UnitId { value: record[0].parse().unwrap(), },
				unit_team : UnitTeam { value: record[1].parse().unwrap(), },
				unit_name : UnitName { value: record[2].to_string(), },
				unit_class : UnitClass { value: record[3].to_string(), },
				pos_x : PosX { value: record[4].parse().unwrap(), }, 
				pos_y : PosY { value: record[5].parse().unwrap(), },
				wt_max : WTMax { value: record[6].parse().unwrap(), },
				wt_current : WTCurrent{ value: record[7].parse().unwrap(), },
				hp_max : HPMax { value: record[8].parse().unwrap(), },
				hp_current : HPCurrent { value: record[9].parse().unwrap(), },
				mp_max : MPMax { value: record[10].parse().unwrap(), },
				mp_current : MPCurrent { value: record[11].parse().unwrap(), },
				str : STR { value: record[12].parse().unwrap(), },
				vit : VIT { value: record[13].parse().unwrap(), },
				int : INT { value: record[14].parse().unwrap(), },
				men : MEN { value: record[15].parse().unwrap(), },
				agi : AGI { value: record[16].parse().unwrap(), },
				dex : DEX { value: record[17].parse().unwrap(), },
				luk : LUK { value: record[18].parse().unwrap(), },
				dir: DIR { direction: Direction::from_string(record[20].to_string()), },
				movement_range: MovementRange { value: record[21].parse().unwrap(), },
				attack_range: AttackRange { value: record[22].parse().unwrap(), },
				attack_type: AttackType::from_string(record[23].to_string()),
			},
			Unit,
			UnitActions { unit_actions: Default::default(), processing_unit_action: false, },
			Pos {
				x: record[4].parse().unwrap(),
				y: record[5].parse().unwrap(),
			},
			MoveActions { move_actions: Vec::new(), },
		)).id();
		
		let mut path_string: String = record[19].to_string();
		path_string.push_str("_east.png");
		map[record[4].parse::<usize>().unwrap()][record[5].parse::<usize>().unwrap()].2.push(entity_id);
	}
	
	info!("DEBUG: Finished spawning units.");
	info!("DEBUG: Starting battle on server...");
	info!("DEBUG: Setting GameState to WaitTurn.");
	next_state.set(GameState::WaitTurn);
	info!("DEBUG: Set GameState to WaitTurn.");
}

// Prototype
fn handle_unit_death(
mut commands: Commands,
mut map_query: Query<&mut Map>,
unit_query: Query<(Entity, &UnitId, &Pos, &HPCurrent)>,
game: Res<Game>,
mut next_state: ResMut<NextState<GameState>>,
) {
	for (entity, unit_id, pos, hp_current) in unit_query.iter() {
		if hp_current.value == 0 {
			// Remove unit.
			let mut map = &mut map_query.single_mut().map;
			map[pos.x][pos.y].2.remove(0);
			
			info!("DEBUG: Unit {} has died. Removing it...", unit_id.value);
			commands.entity(entity).despawn();
			
			// If that unit has the current turn, end it.
			if unit_id.value == game.current_unit {
				// Set GameState to WaitTurn.
				info!("DEBUG: Setting GameState to WaitTurn...");
				next_state.set(GameState::WaitTurn);
				info!("DEBUG: Set GameState to WaitTurn.");
			}
		}
	}
}

// Utility
fn find_path(map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>>, start: Pos, destination: Pos) -> Option<Vec<Pos>> {
    // Define a heuristic function that estimates the distance between two positions.
    // In this case, we use the Manhattan distance (taxicab distance).
    let heuristic = |pos: &Pos| -> usize {
        (pos.x as isize - destination.x as isize).abs() as usize
            + (pos.y as isize - destination.y as isize).abs() as usize
    };

	// Use RefCell to wrap the map so that it can be mutated inside the closure.
    let map_cell = RefCell::new(map);

    // Define a function that returns the valid neighboring positions of a given position.
    let neighbors = |pos: &Pos| -> Vec<(Pos, usize)> {
		// Access the map using borrow_mut() to allow mutation.
		let mut map = map_cell.borrow_mut();
    
        // Add logic to get the valid neighboring positions based on your map layout.
        // For example, avoid diagonal moves and ensure the position is within the map bounds.
        // For simplicity, let's assume you have a function called `get_valid_neighbors`.
        get_valid_neighbors((&mut map).to_vec(), *pos)
    };

    // Use the `astar` function from the pathfinding library to find the path.
    let result = astar(&start, neighbors, heuristic, |&pos| pos == destination);

    if let Some((path, _)) = result {
        Some(path)
    } else {
        None
    }
}

// Utility
fn get_valid_neighbors(map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>>, pos: Pos) -> Vec<(Pos, usize)> {
	let mut neighbors: Vec<(Pos, usize)> = Vec::new(); 
	
	// Check if tile is at North edge.
	if pos.y == map[0].len() - 1 {
		// Tile is at North edge. Don't add North neighbor.
	} else {
		// Tile is not at North edge. 
		// Check if there's a unit on the tile.
		// If there isn't, add North neighbor.
		if map[pos.x][pos.y + 1].2.len() == 0 {
			neighbors.push((Pos { x: pos.x, y: pos.y + 1, }, 0));
		}
	}
	
	// Check if tile is at South edge.
	if pos.y == 0 {
		// Tile is at South edge. Don't add South neighbor.
	} else {
		// Tile is not at South edge.
		// Check if there's a unit on the tile.
		// If there isn't, add South neighbor.
		if map[pos.x][pos.y - 1].2.len() == 0 {
			neighbors.push((Pos { x: pos.x, y: pos.y - 1, }, 0));
		}
	}
	
	// Check if tile is at East edge.
	if pos.x == map.len() - 1 {
		// Tile is at East edge. Don't add East neighbor.
	} else {
		// Tile is not at East edge.
		// Check if there's a unit on the tile.
		// If there isn't, add East neighbor.
		if map[pos.x + 1][pos.y].2.len() == 0 {
			neighbors.push((Pos { x: pos.x + 1, y: pos.y, }, 0));
		}
	} 
	
	// Check if tile is at West edge.
	if pos.x == 0 {
		// Tile is at West edge. Don't add West neighbor.
	} else {
		// Tile is not at West edge.
		// Check if there's a unit on the tile.
		// If there isn't, add West neighbor.
		if map[pos.x - 1][pos.y].2.len() == 0 {
			neighbors.push((Pos { x: pos.x - 1, y: pos.y, }, 0));
		}
	}
	
	return neighbors;
}

use std::collections::HashSet;

// Prototype
fn find_possible_movements(map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>>, start: Pos, mut movement_range: isize) -> Vec<Pos> {
    let mut possible_tiles_vec = Vec::new();
    movement_range -= 1;
	
	let mut visited_tiles = HashSet::new();
	
	if movement_range >= 0 {
		// Get neighbors.
		let neighbors = get_valid_neighbors(map.clone(), start);
		for neighbor in &neighbors {
			if visited_tiles.insert(neighbor.0) {
				possible_tiles_vec.push(neighbor.0);
				let mut recursive_possible_tiles = find_possible_movements(map.clone(), neighbor.0, movement_range);
				
				for possible_tile in recursive_possible_tiles {
					if !possible_tiles_vec.contains(&possible_tile) {
						possible_tiles_vec.push(possible_tile);
					}
				}
			}
		}
    }

    possible_tiles_vec
}

// Utility
fn get_valid_attack_neighbors(map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>>, pos: Pos) -> Vec<(Pos, usize)> {
	let mut neighbors: Vec<(Pos, usize)> = Vec::new(); 
	
	// Check if tile is at North edge.
	if pos.y == map[0].len() - 1 {
		// Tile is at North edge. Don't add North neighbor.
	} else {
		// Tile is not at North edge. 
		neighbors.push((Pos { x: pos.x, y: pos.y + 1, }, 0));
	}
	
	// Check if tile is at South edge.
	if pos.y == 0 {
		// Tile is at South edge. Don't add South neighbor.
	} else {
		// Tile is not at South edge.
		neighbors.push((Pos { x: pos.x, y: pos.y - 1, }, 0));
	}
	
	// Check if tile is at East edge.
	if pos.x == map.len() - 1 {
		// Tile is at East edge. Don't add East neighbor.
	} else {
		// Tile is not at East edge.
		neighbors.push((Pos { x: pos.x + 1, y: pos.y, }, 0));
	} 
	
	// Check if tile is at West edge.
	if pos.x == 0 {
		// Tile is at West edge. Don't add West neighbor.
	} else {
		// Tile is not at West edge.
		neighbors.push((Pos { x: pos.x - 1, y: pos.y, }, 0));
	}
	
	return neighbors;
}

// Prototype
fn find_possible_attacks(map: Vec<Vec<(usize, TileType, Vec<Entity>, Vec<Entity>)>>, start: Pos, mut attack_range: isize, attack_type: AttackType) -> Vec<Pos> {
    let mut possible_tiles_vec = Vec::new();
    
    match attack_type {
		AttackType::Melee => {
			// Get neighbors.
			let neighbors = get_valid_attack_neighbors(map.clone(), start);
			for neighbor in &neighbors {
				possible_tiles_vec.push(neighbor.0);
			}
			
			for neighbor in &neighbors {
				for i in 1..attack_range {
					// If there is an unit on the tile, don't search for more neighbors.
					if map[neighbor.0.x][neighbor.0.y].2.len() > 0 {
						break;
					}
					// Else...
					// Add next neighbor.
					let neighbors = get_valid_attack_neighbors(map.clone(), neighbor.0);
					for neighbor in neighbors {
						if (neighbor.0.x == start.x || neighbor.0.y == start.y) {
							if !(neighbor.0.x == start.x && neighbor.0.y == start.y) {
								if !(possible_tiles_vec.contains(&neighbor.0)) {
									possible_tiles_vec.push(neighbor.0);
								}
							}
						}
					}
				}			
			}
		},
		AttackType::Ranged => {
			attack_range -= 1;
			
			let mut visited_tiles = HashSet::new();
			
			if attack_range >= 0 {
				// Get neighbors.
				let neighbors = get_valid_attack_neighbors(map.clone(), start);
				for neighbor in &neighbors {
					if visited_tiles.insert(neighbor.0) {
						possible_tiles_vec.push(neighbor.0);
						let mut recursive_possible_tiles = find_possible_attacks(map.clone(), neighbor.0, attack_range, attack_type);
						
						for possible_tile in recursive_possible_tiles {
							if !possible_tiles_vec.contains(&possible_tile) {
								possible_tiles_vec.push(possible_tile);
							}
						}
					}
				}
			}
		},
		_ => {
			empty_system();
		},
    }
    
    

    possible_tiles_vec
}

fn empty_system() {

}