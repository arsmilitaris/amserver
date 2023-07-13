// (C) Copyright 2023 Ars Militaris Dev

use bevy::{prelude::*, render::camera::ScalingMode, log::LogPlugin, diagnostic::LogDiagnosticsPlugin, diagnostic::FrameTimeDiagnosticsPlugin, window::*};

use bevy::ecs::system::SystemParam;
use bevy::ecs::world::World;

use gridly_grids::VecGrid;
use gridly::prelude::*;

use std::fs;

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

pub mod kafka_am;

#[derive(Serialize, Deserialize)]
enum ClientMessage {
	GetClientId,
	StartGame,
	LoadingComplete,
	WaitTurnComplete,
	Wait,
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
}

enum UnitAction {
	Move {
		destination: Pos,
	},
	Talk {
		message: String,
	},
}

// COMPONENTS

#[derive(Component)]
struct NakedSwordsman {

}

#[derive(Component)]
struct MoveAction {
	destination: Pos,
}

#[derive(Component)]
struct TalkAction {
	message: String,
}

#[derive(Component)]
struct UnitActions {
	unit_actions: Vec<(UnitAction, f32)>,
	processing_unit_action: bool,
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

#[derive(Component, Clone, Copy)]
struct Pos {
	x: usize,
	y: usize,
}

#[derive(Component, Clone, Serialize, Deserialize)]
struct UnitId { value: usize, }

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

#[derive(Resource, Default)]
struct Game {
	current_unit: usize,
	has_started: bool,
}

#[derive(Resource, Default)]
struct Timers {
	two_second_timer: Timer,
	six_second_timer: Timer,
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
		.add_systems(OnEnter(GameState::MainMenu), start_listening)
		.add_systems(Update,
						handle_client_messages
							.run_if(in_state(GameState::MainMenu))
		)
		.add_systems(Update,
						(read_map_system, setup_map_system, read_battle_system, generate_units_system, place_units_on_map_system)
							.run_if(in_state(GameState::Loading))
		)
		.add_systems(Update, 
						handle_loading_complete_messages
							.run_if(in_state(GameState::ClientsLoading))
		)
		.add_systems(OnEnter(GameState::Loading), on_enter_loading_state)
		//.add_systems(OnEnter(GameState::LoadMap), setup_grid_system)
		//.add_systems(OnEnter(GameState::LoadMap), setup_camera_system)
		//.add_systems(OnEnter(GameState::LoadMap), (apply_deferred, setup_text_system)
		//	.chain()
		//	.after(setup_grid_system)
		//)
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

// Server
fn read_map_system(mut events: EventReader<GameStartEvent>, mut events2: EventWriter<MapReadEvent>) {
	
	for event in events.iter() {
		//info!("DEBUG: Reading map file...");
	
		// This function has a bug when using the '\\' as the path separator.
		// It won't work on Linux.
		let file_contents = fs::read_to_string("src/map.txt").unwrap();
		
		//info!("DEBUG: Read map file.");
		
		// Separate map into lines.
		let map_lines: Vec<&str> = file_contents.split('\n').collect();
		//info!("DEBUG: Map line 1 is: \n{}", map_lines[0]);
		
		// Separate lines and build 2D-array.
		info!("DEBUG: Starting to build 2D array of map...");
		let mut map: Vec<Vec<String>> = Vec::new();
		for i in 0..map_lines.len() {
			let mut map_line: Vec<String> = Vec::new();
			let line = map_lines[i];
			let line_splitted: Vec<&str> = line.split(' ').collect();
			for j in 0..line_splitted.len() {
				let map_cell = line_splitted[j].to_owned();
				map_line.push(map_cell);
			}
			map.push(map_line);
		}
		info!("DEBUG: Finished building 2D array of map.");
		
		//info!("DEBUG: Printing map file...");
		//info!("{}", file_contents);
		//info!("DEBUG: Printed map file...");
		
		events2.send(MapReadEvent {
						map: map,
					});
	}
}

// Client & Server
fn setup_map_system(mut events: EventReader<MapReadEvent>, mut events2: EventWriter<MapSetupEvent>, mut commands: Commands, asset_server: Res<AssetServer>) {
	
	for event in events.iter() {
		
		info!("DEBUG: Starting to set up map in the ECS World...");
		info!("DEBUG: Finished setting up map in the ECS World.");
		events2.send(MapSetupEvent);
	}
}

// Server
fn read_battle_system(mut events: EventReader<MapSetupEvent>, mut events2: EventWriter<UnitsReadEvent>) {
	for event in events.iter() {
		// This function has a bug when using the '\\' as the path separator.
		// It won't work on Linux.
		let mut rdr = Reader::from_path("src/the_patrol_ambush_data.csv").unwrap();
		let mut records: Vec<StringRecord> = Vec::new();
		for result in rdr.records(){
			let record = result.unwrap();
			//info!("{:?}", record);
			records.push(record);
		}
		events2.send(UnitsReadEvent {
							units: records,
						});
	}
}

// Server
fn generate_units_system(mut events: EventReader<UnitsReadEvent>, mut events2: EventWriter<UnitsGeneratedEvent>, mut commands: Commands) {
	
	for event in events.iter() {
		// For each record, create an Entity for an unit.
		let records = &event.units;
		for record in records {
			info!("DEBUG: Creating new unit...");
			commands.spawn((
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
				},
				Unit,
			));
		}
		events2.send(UnitsGeneratedEvent);
	}
}

// Server
fn place_units_on_map_system(mut events: EventReader<UnitsGeneratedEvent>, unit_positions: Query<(&UnitId, &PosX, &PosY)>, mut tiles: Query<(&Tile, &Pos, &mut Text)>, mut commands: Commands, mut next_state: ResMut<NextState<GameState>>) {
	
	for event in events.iter() {
		info!("DEBUG: Starting to place units on map...");
		info!("DEBUG: Finished placing units on map.");

		info!("DEBUG: Setting GameState to WaitTurn...");
		//info!("DEBUG: Setting GameState to ClientsLoading...");
		//commands.insert_resource(NextState(GameState::WaitTurn));	
		//next_state.set(GameState::ClientsLoading);
		//info!("DEBUG: Set GameState to ClientsLoading.");
		next_state.set(GameState::WaitTurn);
		info!("DEBUG: Set GameState to WaitTurn.");
	}
}

// Server
fn wait_turn_system(mut units: Query<(&mut WTCurrent, &WTMax, &UnitId, &UnitTeam)>, mut game: ResMut<Game>, mut commands: Commands, mut server: ResMut<Server>, mut next_state: ResMut<NextState<GameState>>) {
	
	let endpoint = server.endpoint_mut();
	
	// Decrease all units WT. If WT equals 0, set the unit as the current unit turn.
	for (mut wt_current, wt_max, unit_id, unit_team) in units.iter_mut() {
		if wt_current.value == 0 {
			info!("DEBUG: It is now unit {} turn.", unit_id.value);
			game.current_unit = unit_id.value;
			
			// Send PlayerTurn message.
			info!("DEBUG: Sending Player Turn message...");
			endpoint.broadcast_message(ServerMessage::PlayerTurn { client_id: unit_team.value as u64, current_unit: unit_id.value, }).unwrap();
			info!("DEBUG: Sent Player Turn message.");
			
			info!("DEBUG: Setting GameState to Battle..."); 
			//commands.insert_resource(NextState(GameState::Battle));
			next_state.set(GameState::Battle);
			info!("DEBUG: Set GameState to Battle.");
		} else {
			wt_current.value = wt_current.value - 1;
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
mut units: Query<(&mut WTCurrent, &WTMax, &UnitTeam)>,
mut next_state: ResMut<NextState<GameState>>,
) {
	let mut endpoint = server.endpoint_mut();

	for client_id in endpoint.clients() {
		while let Ok(Some(message)) = endpoint.receive_message_from::<ClientMessage>(client_id) {
			match message {
				ClientMessage::WaitTurnComplete => {
					info!("DEBUG: Received WaitTurnComplete message.");
					
					// Get the current unit's team.
					for (mut wt_current, wt_max, unit_team) in units.iter_mut() {
						//info!("DEBUG: WT Current is {}.", wt_current.value);
						if wt_current.value == 0 {
							info!("DEBUG: The current unit's team is {}.", unit_team.value);
							//info!("DEBUG: Sending PlayerTurn message...");
							//endpoint.send_message(client_id, ServerMessage::PlayerTurn { client_id: unit_team.value as u64, }).unwrap();
							//info!("DEBUG: Sent PlayerTurn message.");
							break;
						}
					}
				},
				ClientMessage::Wait => {
					info!("DEBUG: Received Wait message.");
					
					// Send Wait message.
					info!("DEBUG: Sending Wait message...");
					endpoint.send_message(client_id, ServerMessage::Wait).unwrap();
					info!("DEBUG: Sent Wait message.");
					
					// Reset the current unit's WT.
					for (mut wt_current, wt_max, unit_team) in units.iter_mut() {
						if wt_current.value == 0 {
							wt_current.value = wt_max.value;
							break;
						}
					}
					info!("DEBUG: Setting GameState to WaitTurn...");
					//commands.insert_resource(NextState(GameState::WaitTurn));
					next_state.set(GameState::WaitTurn);
					info!("DEBUG: Set GameState to WaitTurn...");
				},
				_ => { empty_system(); },
			}
		}
	}
}

// Server
fn setup_game_resource_system(mut commands: Commands) {
	commands.insert_resource(Game {
		current_unit: 0,
		has_started: false,
	});
}

// Server
fn start_listening(mut server: ResMut<Server>) {
	server
		.start_endpoint(
			ServerConfiguration::from_string("127.0.0.1:6000").unwrap(),
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
) {
	let endpoint = server.endpoint_mut();
	
	for client_id in endpoint.clients() {
		while let Ok(Some(message)) = endpoint.receive_message_from::<ClientMessage>(client_id) {
			match message {
				ClientMessage::LoadingComplete => {
					info!("DEBUG: Received LoadingComplete message from client {}.", client_id);
					endpoint.broadcast_message(ServerMessage::StartGame2).unwrap();
					info!("DEBUG: Sent StartGame2 message to clients.");
					
					// Set GameState to Loading.
					info!("DEBUG: Setting GameState to Loading...");
					next_state.set(GameState::Loading);
					info!("DEBUG: Set GameState to Loading.");
				},
				_ => { empty_system() }
			}
		}	
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

fn empty_system() {

}