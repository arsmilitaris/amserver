// (C) Copyright 2023 Ars Militaris Dev

use bevy::{prelude::*, render::camera::ScalingMode, diagnostic::LogDiagnosticsPlugin, diagnostic::FrameTimeDiagnosticsPlugin};

use iyes_loopless::prelude::*;

use gridly_grids::VecGrid;
use gridly::prelude::*;

use std::fs;

use csv::Reader;
use csv::StringRecord;

use kafka::producer::{Producer, Record, RequiredAcks};

use bevy_quinnet::{
    server::{
        certificate::CertificateRetrievalMode, ConnectionLostEvent, Endpoint, QuinnetServerPlugin,
        Server, ServerConfigurationData,
    },
    shared::ClientId,
};

use serde::{Deserialize, Serialize};

pub mod kafka_am;

#[derive(Serialize, Deserialize)]
enum ClientMessage {
	GetClientId,
	StartGame,
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
	PlayerTurn {
		client_id: ClientId,
		current_unit: usize,
	},
	WaitTurn {
		wait_turns: Vec<(UnitId, WTCurrent)>,
	},
	Wait,
}

// COMPONENTS

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

#[derive(Component)]
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

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum GameState {
	MainMenu,
	Loading,
	LoadMap,
	Battle,
	WaitTurn,
}

// EVENTS

struct GameStartEvent;

struct MapReadEvent {
	pub map: Vec<Vec<String>>,
}

struct MapSetupEvent;

struct UnitsReadEvent {
	pub units: Vec<StringRecord>,
}

struct UnitsGeneratedEvent;

// RESOURCES

#[derive(Resource, Default)]
struct Game {
	current_unit: usize,
	has_started: bool,
}


// Client & Server
fn main() {
	
    App::new()
		.add_plugins(DefaultPlugins)
		//.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
		.add_plugin(QuinnetServerPlugin::default())
		.add_loopless_state(GameState::MainMenu)
		.add_event::<GameStartEvent>()
		.add_event::<MapReadEvent>()
		.add_event::<MapSetupEvent>()
		.add_event::<UnitsReadEvent>()
		.add_event::<UnitsGeneratedEvent>()
		.init_resource::<Game>()
		.add_enter_system(GameState::MainMenu, start_listening)
		.add_system_set(
			ConditionSet::new()
				.run_in_state(GameState::MainMenu)
				.with_system(start_game_system)
				.with_system(handle_client_messages)
				.into()
		)
		.add_system_set(
			ConditionSet::new()
				.run_in_state(GameState::Loading)
				.with_system(read_map_system)
				.with_system(setup_map_system)
				.with_system(read_battle_system)
				.with_system(generate_units_system)
				.with_system(place_units_on_map_system)
				.into()
		)
		.add_enter_system(GameState::LoadMap, setup_grid_system)
		.add_enter_system(GameState::LoadMap, setup_camera_system)
		.add_system(setup_text_system
			.run_in_state(GameState::LoadMap)
			.run_if(grid_already_setup)
			.run_if_not(text_already_setup)
		)
		.add_system(z_order_system
			.run_in_state(GameState::LoadMap)
			.run_if(text_already_setup)
		)
		.add_system(move_camera_system
			.run_in_state(GameState::LoadMap)
		)
		.add_system(spawn_gaul_warrior
			.run_in_state(GameState::LoadMap)
			.run_if(grid_already_setup)
			.run_if_not(warrior_already_spawned)
		)
		.add_system(move_gaul_warrior
			.run_in_state(GameState::LoadMap)
			.run_if(warrior_already_spawned)
		)
		.add_system(test_system_2
			.run_in_state(GameState::LoadMap)
			.run_if(warrior_already_spawned)
		)
		.add_enter_system(GameState::Battle, setup_cursor_system.run_if_not(cursor_already_spawned))
		.add_system_set(
			ConditionSet::new()
				.run_in_state(GameState::Battle)
				.with_system(move_cursor_system)
				.with_system(end_turn_system)
				.with_system(handle_wait_turn_completed)
				//.with_system(handle_client_messages)
				//.with_system(handle_wait_client_message)
				.into()
		)
		.add_system_set(
			ConditionSet::new()
				.run_in_state(GameState::WaitTurn)
				.with_system(wait_turn_system)
				.with_system(handle_wait_turn_completed)
				.into()
		)
		.add_exit_system(GameState::WaitTurn, on_complete_wait_turn)
		.run();
}

// SYSTEMS

// Server
fn read_map_system(mut events: EventReader<GameStartEvent>, mut events2: EventWriter<MapReadEvent>) {
	
	for event in events.iter() {
		//info!("DEBUG: Reading map file...");
	
		let file_contents = fs::read_to_string("C:\\Users\\Isabel\\Desktop\\raul\\amserver\\src\\map.txt").unwrap();
		
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
		// Spawn camera.
		commands.spawn(
			Camera2dBundle::default()
		);
		
		let map = &event.map;
		
		info!("DEBUG: Starting to set up map in the ECS World...");
		// For each cell in map, generate a 2D text and position it.
		for i in 0..map.len() {
			for j in 0..map[i].len() {
			
				// Compute position.
				let i_as_float = i as f32;
				let j_as_float = j as f32;
			
				// Spawn text.
				commands.spawn((
					TextBundle::from_section(
						map[i][j].as_str(),
						TextStyle {
							font: asset_server.load("fonts\\FiraSans-Bold.ttf"),
							font_size: 80.0,
							color: Color::WHITE,
						},
					)
					.with_text_alignment(TextAlignment::TOP_CENTER)
					.with_style(Style {
						position_type: PositionType::Absolute,
						position: UiRect {
							top: Val::Px(60.0 * i_as_float),
							right: Val::Px(60.0 * j_as_float),
							..default()
						},
						max_size: Size {
							width: Val::Px(100.0),
							height: Val::Px(300.0),
						},
						..default()
					}),
					Tile,
					Pos {
						x: i,
						y: j,
					},
				));
			}
		}
		info!("DEBUG: Finished setting up map in the ECS World.");
		events2.send(MapSetupEvent);
	}
}

// Server
fn read_battle_system(mut events: EventReader<MapSetupEvent>, mut events2: EventWriter<UnitsReadEvent>) {
	for event in events.iter() {
		let mut rdr = Reader::from_path("C:\\Users\\Isabel\\Desktop\\raul\\amserver\\src\\the_patrol_ambush_data.csv").unwrap();
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
fn place_units_on_map_system(mut events: EventReader<UnitsGeneratedEvent>, unit_positions: Query<(&UnitId, &PosX, &PosY)>, mut tiles: Query<(&Tile, &Pos, &mut Text)>, mut commands: Commands) {
	
	for event in events.iter() {
		info!("DEBUG: Starting to place units on map...");
		// For each Unit...
		for (unit_id, unit_position_x, unit_position_y) in unit_positions.iter() {
			// Get the unit X and Y coordinates.
			let x = unit_position_x.value;
			let y = unit_position_y.value;
			
			// Get the tile at coordinates (x, y)
			for (tile, pos, mut text) in tiles.iter_mut() {
				if pos.x == x && pos.y == y {
					// Assign unit ID to tile.
					info!("DEBUG: Assigning unit ID to tile.");
					text.sections[0].value = unit_id.value.to_string();
				}
			}
		}
		info!("DEBUG: Finished placing units on map.");
		
		info!("DEBUG: Setting GameState to WaitTurn...");
		commands.insert_resource(NextState(GameState::WaitTurn));	
		info!("DEBUG: Set GameState to WaitTurn.");
	}
}

// Client & Server
fn start_game_system(mut input: ResMut<Input<KeyCode>>, mut events: EventWriter<GameStartEvent>, mut commands: Commands) {
	if input.just_pressed(KeyCode::Space) {
		info!("DEBUG: Setting GameState to LoadMap...");
		commands.insert_resource(NextState(GameState::LoadMap));
		info!("DEBUG: Set GameState to LoadMap.");
        //events.send(GameStartEvent);
    } 
}

// Client
fn setup_cursor_system(mut commands: Commands, mut tiles: Query<(&Tile, &Pos, &mut Text)>) {
	
	info!("DEBUG: setup_cursor_system running...");
	// Setup cursor.
	for (tile, pos, mut text) in tiles.iter_mut() {
		if pos.x == 5 && pos.y == 5 {
			// Place cursor at the center of the map.
			info!("DEBUG: Found tile at coordinates 5, 5. Placing cursor there.");
			
			// Build cursor string.
			let mut cursor = "[".to_owned();
			cursor.push_str(&text.sections[0].value);
			cursor.push_str("]");
			
			// Assign cursor string to map.
			text.sections[0].value = cursor;
			
			// Spawn the cursor Entity.
			commands.spawn(Cursor { 
								x: 5,
								y: 5,
							});
		}
	}
}

// Client
fn move_cursor_system(input: Res<Input<KeyCode>>, mut cursors: Query<&mut Cursor>, mut tiles: Query<(&Tile, &Pos, &mut Text)>) {
	
	// Get cursor current position.
	let mut cursor_position_x = 0;
	let mut cursor_position_y = 0;
	for cursor in cursors.iter_mut() {
		cursor_position_x = cursor.x;
		cursor_position_y = cursor.y;
	}
	
	if input.just_pressed(KeyCode::A) {
		
		// Save the previous cursor position to later be used in removing the cursor.
		let cursor_previous_x = cursor_position_x;
		let cursor_previous_y = cursor_position_y;
		
		if cursor_position_y == 9 {
			info!("DEBUG: You can't move the cursor there.");
		} else {
			
		
			// Find tile to the left of cursor.
			for (tile, pos, mut text) in tiles.iter_mut() {
				if pos.x == cursor_position_x && pos.y == cursor_position_y + 1 {
					// Move the cursor to the new position.
					info!("DEBUG: Found tile at coordinates {}, {}.", pos.x, pos.y);
					
					// Build cursor string.
					let mut cursor_string = "[".to_owned();
					cursor_string.push_str(&text.sections[0].value);
					cursor_string.push_str("]");
					text.sections[0].value = cursor_string;
					
					
					
					
					// Update the cursor Entity.
					for mut cursor in cursors.iter_mut() {
						cursor.x = pos.x;
						cursor.y = pos.y;

					}
				}
			}
			
			// Remove the cursor from the previous tile.
			for cursor in cursors.iter_mut() {
				for (tile, pos, mut text) in tiles.iter_mut() {
					if pos.x == cursor_previous_x && pos.y == cursor_previous_y {
						// Remove [ and ] from tile.
						let mut tile_string = &text.sections[0].value;
						let mut tile_string_split = tile_string.split("[");
						let vec = tile_string_split.collect::<Vec<&str>>();
						let mut tile_string_split_2 = vec[1].split("]");
						let vec2 = tile_string_split_2.collect::<Vec<&str>>();
						let new_tile_string = vec2[0];
						
						// Assign new string to tile.
						text.sections[0].value = new_tile_string.to_string();
					}
				}
			}
			
			info!("DEBUG: Moving the cursor...");
			
		}
	} else if input.just_pressed(KeyCode::D) {
	
		// Save the previous cursor position to later be used in removing the cursor.
		let cursor_previous_x = cursor_position_x;
		let cursor_previous_y = cursor_position_y;
		
		if cursor_position_y == 0 {
			info!("DEBUG: You can't move the cursor there.");
		} else {
			
		
			// Find tile to the left of cursor.
			for (tile, pos, mut text) in tiles.iter_mut() {
				if pos.x == cursor_position_x && pos.y == cursor_position_y - 1 {
					// Move the cursor to the new position.
					info!("DEBUG: Found tile at coordinates {}, {}.", pos.x, pos.y);
					
					// Build cursor string.
					let mut cursor_string = "[".to_owned();
					cursor_string.push_str(&text.sections[0].value);
					cursor_string.push_str("]");
					text.sections[0].value = cursor_string;
					
					
					
					
					// Update the cursor Entity.
					for mut cursor in cursors.iter_mut() {
						cursor.x = pos.x;
						cursor.y = pos.y;

					}
				}
			}
			
			// Remove the cursor from the previous tile.
			for cursor in cursors.iter_mut() {
				for (tile, pos, mut text) in tiles.iter_mut() {
					if pos.x == cursor_previous_x && pos.y == cursor_previous_y {
						// Remove [ and ] from tile.
						let mut tile_string = &text.sections[0].value;
						let mut tile_string_split = tile_string.split("[");
						let vec = tile_string_split.collect::<Vec<&str>>();
						let mut tile_string_split_2 = vec[1].split("]");
						let vec2 = tile_string_split_2.collect::<Vec<&str>>();
						let new_tile_string = vec2[0];
						
						// Assign new string to tile.
						text.sections[0].value = new_tile_string.to_string();
					}
				}
			}
			
			info!("DEBUG: Moving the cursor...");
			
		}
	} else if input.just_pressed(KeyCode::W) {
	
		// Save the previous cursor position to later be used in removing the cursor.
		let cursor_previous_x = cursor_position_x;
		let cursor_previous_y = cursor_position_y;
		
		if cursor_position_x == 0 {
			info!("DEBUG: You can't move the cursor there.");
		} else {
			
		
			// Find tile to the left of cursor.
			for (tile, pos, mut text) in tiles.iter_mut() {
				if pos.x == cursor_position_x - 1 && pos.y == cursor_position_y {
					// Move the cursor to the new position.
					info!("DEBUG: Found tile at coordinates {}, {}.", pos.x, pos.y);
					
					// Build cursor string.
					let mut cursor_string = "[".to_owned();
					cursor_string.push_str(&text.sections[0].value);
					cursor_string.push_str("]");
					text.sections[0].value = cursor_string;
					
					
					
					
					// Update the cursor Entity.
					for mut cursor in cursors.iter_mut() {
						cursor.x = pos.x;
						cursor.y = pos.y;

					}
				}
			}
			
			// Remove the cursor from the previous tile.
			for cursor in cursors.iter_mut() {
				for (tile, pos, mut text) in tiles.iter_mut() {
					if pos.x == cursor_previous_x && pos.y == cursor_previous_y {
						// Remove [ and ] from tile.
						let mut tile_string = &text.sections[0].value;
						let mut tile_string_split = tile_string.split("[");
						let vec = tile_string_split.collect::<Vec<&str>>();
						let mut tile_string_split_2 = vec[1].split("]");
						let vec2 = tile_string_split_2.collect::<Vec<&str>>();
						let new_tile_string = vec2[0];
						
						// Assign new string to tile.
						text.sections[0].value = new_tile_string.to_string();
					}
				}
			}
			
			info!("DEBUG: Moving the cursor...");
			
		}
	} else if input.just_pressed(KeyCode::S) {
	
		// Save the previous cursor position to later be used in removing the cursor.
		let cursor_previous_x = cursor_position_x;
		let cursor_previous_y = cursor_position_y;
		
		if cursor_position_x == 9 {
			info!("DEBUG: You can't move the cursor there.");
		} else {
			
		
			// Find tile to the left of cursor.
			for (tile, pos, mut text) in tiles.iter_mut() {
				if pos.x == cursor_position_x + 1 && pos.y == cursor_position_y {
					// Move the cursor to the new position.
					info!("DEBUG: Found tile at coordinates {}, {}.", pos.x, pos.y);
					
					// Build cursor string.
					let mut cursor_string = "[".to_owned();
					cursor_string.push_str(&text.sections[0].value);
					cursor_string.push_str("]");
					text.sections[0].value = cursor_string;
					
					
					
					
					// Update the cursor Entity.
					for mut cursor in cursors.iter_mut() {
						cursor.x = pos.x;
						cursor.y = pos.y;

					}
				}
			}
			
			// Remove the cursor from the previous tile.
			for cursor in cursors.iter_mut() {
				for (tile, pos, mut text) in tiles.iter_mut() {
					if pos.x == cursor_previous_x && pos.y == cursor_previous_y {
						// Remove [ and ] from tile.
						let mut tile_string = &text.sections[0].value;
						let mut tile_string_split = tile_string.split("[");
						let vec = tile_string_split.collect::<Vec<&str>>();
						let mut tile_string_split_2 = vec[1].split("]");
						let vec2 = tile_string_split_2.collect::<Vec<&str>>();
						let new_tile_string = vec2[0];
						
						// Assign new string to tile.
						text.sections[0].value = new_tile_string.to_string();
					}
				}
			}
			
			info!("DEBUG: Moving the cursor...");
			
		}
	}
}

// Server
fn wait_turn_system(mut units: Query<(&mut WTCurrent, &WTMax, &UnitId, &UnitTeam)>, mut game: ResMut<Game>, mut commands: Commands, mut server: ResMut<Server>) {
	
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
			commands.insert_resource(NextState(GameState::Battle));
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
mut units: Query<(&mut WTCurrent, &WTMax, &UnitTeam)>
) {
	let mut endpoint = server.endpoint_mut();

	while let Ok(Some((message, client_id))) = endpoint.receive_message::<ClientMessage>() {
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
				commands.insert_resource(NextState(GameState::WaitTurn));
				info!("DEBUG: Set GameState to WaitTurn...");
            },
            _ => { empty_system(); },
        }
    }
}

// Client
fn end_turn_system(mut input: ResMut<Input<KeyCode>>, mut units: Query<(&mut WTCurrent, &WTMax)>, mut commands: Commands) {
	if input.just_pressed(KeyCode::T) {
		info!("DEBUG: The current unit has ended its turn.");
		info!("DEBUG: Reseting the unit's WT.");
		for (mut wt_current, wt_max) in units.iter_mut() {
			if wt_current.value == 0 {
				wt_current.value = wt_max.value;
				break;
			}
		}
		
		info!("DEBUG: Setting GameState to WaitTurn...");
		commands.insert_resource(NextState(GameState::WaitTurn));
		info!("DEBUG: Set GameState to WaitTurn.");
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
fn message_kafka_system() {
	let mut producer = kafka_am::producer::GameProducer::new().unwrap();
	producer.p.send(&Record::from_value("topic2", String::from("1").as_bytes())).unwrap();
}

// Client
fn receive_kafka_system() {
	let gc = kafka_am::consumer::GameConsumer::new("topic2").unwrap();
	let map = gc.recv();
	println!("DEBUG: {}", map);
}

// Server
fn start_listening(mut server: ResMut<Server>) {
	server
		.start_endpoint(
			ServerConfigurationData::new("127.0.0.1".to_string(), 6000, "0.0.0.0".to_string()),
			CertificateRetrievalMode::GenerateSelfSigned,
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
) {
    let mut endpoint = server.endpoint_mut();
    while let Ok(Some((message, client_id))) = endpoint.receive_message::<ClientMessage>() {
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
					info!("DEBUG: Setting GameState to Loading...");
					commands.insert_resource(NextState(GameState::Loading));
					info!("DEBUG: Set GameState to Loading.");
					events.send(GameStartEvent);
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

// Server
fn handle_wait_client_message(mut server: ResMut<Server>, mut commands: Commands) {
	let mut endpoint = server.endpoint_mut();
	
	while let Ok(Some((message, client_id))) = endpoint.receive_message::<ClientMessage>() {
		match message {
			ClientMessage::Wait => {
				info!("DEBUG: Received Wait message.");
				info!("DEBUG: Sending Wait message...");
				endpoint.broadcast_message(ServerMessage::Wait).unwrap();
				info!("DEBUG: Sent Wait message.");
				
				commands.insert_resource(NextState(GameState::WaitTurn));
			},
			_ => { 
				info!("DEBUG: Received other message.");
			},
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

// Client
fn setup_camera_system(mut commands: Commands) {
	commands.spawn(Camera2dBundle::default());
}

// Client
fn setup_text_system(mut query: Query<&mut Map>, mut commands: Commands, asset_server: Res<AssetServer>) {
	let font = asset_server.load("fonts/FiraSans-Bold.ttf");
	let text_style = TextStyle {
		font,
		font_size: 20.0,
		color: Color::WHITE,
	};
	let text_alignment = TextAlignment::CENTER;
	
	let mut map = query.single_mut();
	for i in 0..map.map.len() {
		for j in 0..map.map[i].len() {
			//let tile_string: String = map.map[i][j].2.to_string();
			
			for k in 0..map.map[i][j].0 {
				let entity_id = commands.spawn((
					//Text2dBundle {
					//	text: Text::from_section(tile_string, text_style.clone()).with_alignment(text_alignment),
					//	transform: Transform::from_xyz((i as f32) * 256.0 / 2.0 - (j as f32) * 256.0 / 2.0, (i as f32) * 128.0 / 2.0 + (j as f32) * 128.0 / 2.0, 0.0),
					//	..default()
					//},
					SpriteBundle {
						texture: asset_server.load("tile.png"),
						transform: Transform::from_xyz((i as f32) * 256.0 / 2.0 - (j as f32) * 256.0 / 2.0, ((i as f32) * 128.0 / 2.0 + (j as f32) * 128.0 / 2.0) * (111.0 / (128.0 / 2.0) - 1.0) + (k as f32) * 30.0, 1.0),
						..default()
					},
					GameText,
				)).id();
				
				map.map[i][j].3.push(entity_id);
			}
		}
	}
	
}

// Client
fn z_order_system(
mut query: Query<&mut Transform, With<GameText>>,
mut unit_query: Query<&mut Transform, (With<Unit>, Without<GameText>)>,
map_query: Query<&Map>,
) {
	//info!("DEBUG: Z-Order system running...");
	let map = &map_query.single().map;
	//info!("DEBUG: Map length is: {}.", map.len());
	
	let mut counter = 0.0;
	let mut counter_2 = 0.0;
	
	for i in (0..map.len()).rev() {
		for j in (0..map[i].len()).rev() {
			counter += 0.00001;
			
			//info!("Tile Entity ID is: {:?}.", map[i][j].3);
			for k in 0..map[i][j].3.len() {
				counter_2 += 0.0000001;
				if let Ok(mut tile_transform) = query.get_mut(map[i][j].3[k]) {
					//info!("DEBUG: Tile position is: {:?}.", tile_transform.translation);
					tile_transform.translation.z = counter + counter_2;
				}
			}
			
			// If there is a unit on the tile, order it.
			if map[i][j].2.len() > 0 {
				// Order unit.
				if let Ok(mut unit_transform) = unit_query.get_mut(map[i][j].2[0]) {
					if let Ok(mut tile_transform) = query.get_mut(map[i][j].3[map[i][j].3.len() - 1]) {
						unit_transform.translation = tile_transform.translation;
						unit_transform.translation.y += 100.0;
						unit_transform.translation.z += 0.0000001;
					}
					//unit_transform.translation.z = counter + counter_2 + 0.0000001;
				}
			}
		}
	}
}

// Client
fn z_unit_order_system(
mut query: Query<&mut Transform, With<Unit>>,
map_query: Query<&Map>,
) {
	let map = &map_query.single().map;

	for i in 0..map.len() {
		for j in 0..map[i].len() {
			if map[i][j].2.len() > 0 {
				// Order unit.
				if let Ok(mut unit_transform) = query.get_mut(map[i][j].2[0]) {
					//if let Ok(mut tile_transform) = 
				}
				
				
			}
		}
	}

	for unit_transform in query.iter_mut() {
		
	}
}

// Client
fn move_camera_system(
mut camera_transform_query: Query<&mut Transform, With<Camera>>,
windows: Res<Windows>,
) {
	let window = windows.get_primary().unwrap();
	
	let mut camera_transform = camera_transform_query.single_mut();
	
	if let Some(_position) = window.cursor_position() {
		// Cursor is inside the window.
		//info!("DEBUG: Cursor position is: {:?}.", _position);
		//info!("DEBUG: Window width is: {:?}.", window.width());
		//info!("DEBUG: Window height is: {:?}.", window.height());
		
		if _position.x <= 0.0 + 20.0 {
			//info!("DEBUG: Camera translation is: {:?}.", camera_transform.translation);
			camera_transform.translation -= Vec3::X * 3.0;
		} else if _position.x >= window.width() - 20.0 {
			camera_transform.translation += Vec3::X * 3.0;
		}
		
		if _position.y <= 0.0 + 20.0 {
			//info!("DEBUG: Camera translation is: {:?}.", camera_transform.translation);
			camera_transform.translation -= Vec3::Y * 3.0;
		} else if _position.y >= window.height() - 20.0 {
			camera_transform.translation += Vec3::Y * 3.0;
		}
		
	} else {
		// Cursor is not inside the window.
	}
}

// Client & Server
fn spawn_gaul_warrior(
mut commands: Commands,
mut map_query: Query<&mut Map>,
asset_server: Res<AssetServer>,
) {
	let mut map = &mut map_query.single_mut().map;
	
	let entity_id = commands.spawn((
		SpriteBundle {
			texture: asset_server.load("gaul_warrior.png"),
			transform: Transform::from_xyz((0 as f32) * 256.0 / 2.0 - (0 as f32) * 256.0 / 2.0, (0 as f32) * 128.0 / 2.0 + (0 as f32) * 128.0 / 2.0 + (map[0][0].0 as f32) * 15.0 + 100.0, 1.0),
			..default()
		},
		Unit,
		Pos {
			x: 0,
			y: 0,
		},
	)).id();
	
	map[0][0].2.push(entity_id);
}

// Prototype
fn move_gaul_warrior(
mut map_query: Query<&mut Map>,
mut unit_query: Query<(Entity, &mut Pos), With<Unit>>,
mut input: ResMut<Input<KeyCode>>,
) {
	let mut map = &mut map_query.single_mut().map;
	
	if input.just_pressed(KeyCode::W) {
		info!("DEBUG: W pressed.");
		info!("DEBUG: unit_query length is: {}.", unit_query.iter_mut().len());
		for (entity_id, mut pos) in unit_query.iter_mut() {
			//info!("
			map[pos.x][pos.y + 1].2.push(entity_id);
			map[pos.x][pos.y].2.pop();
			pos.y += 1;
		}
	}
	
}

// Utility
fn print_gaul_warrior(
query: Query<&Transform, With<Unit>>,
) {
	let unit_transform = query.single();
	info!("DEBUG: Unit translation is: {:?}.", unit_transform.translation);
}

// Client & Server
fn grid_already_setup(query: Query<&Map>) -> bool {
	if query.iter().len() == 0 {
		return false;
	} else {
		return true;
	}
}

// Client
fn text_already_setup(query: Query<&GameText>) -> bool {
	if query.iter().len() == 0 {
		return false;
	} else {
		return true;
	}
}

// Client & Server
fn warrior_already_spawned(query: Query<&Unit>) -> bool {
	if query.iter().len() == 0 {
		return false;
	} else {
		return true;
	}
}

fn test_system(text: Query<&Transform, With<GameText>>) {
	info!("DEBUG: Text is at position {}.", text.single().translation);
}

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

// Server
fn cursor_already_spawned(cursors: Query<&Cursor>) -> bool {
	let mut cursor_spawned = false;
	for cursor in cursors.iter() {
		cursor_spawned = true;
	}
	return cursor_spawned;
}

fn empty_system() {

}