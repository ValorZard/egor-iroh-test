use std::collections::{HashMap, HashSet};

use game_core::{
    DEFAULT_PLAYER_ID, PlayerId, PlayerPosition, ReliableClientMessage, ReliableServerMessage,
    UnreliableClientMessage, UnreliableServerMessage,
    client::{Client, run_client},
    server::{self, Server, run_server},
};
use hecs::{Entity, World};

#[derive(Clone, Debug)]
pub struct InputData {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub position: PlayerPosition,
    pub width: f32,
    pub height: f32,
    pub is_local: bool,
}

pub enum NetworkState {
    ClientConnection(Client, Entity),
    ServerConnection(Server, Option<Entity>),
}

pub struct GameState {
    pub network_state: Option<NetworkState>,
    world: World,
    remote_player_map: HashMap<PlayerId, hecs::Entity>,
    async_runtime: tokio::runtime::Runtime,
}

impl Default for GameState {
    fn default() -> Self {
        GameState {
            network_state: None,
            world: World::new(),
            remote_player_map: HashMap::new(),
            async_runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }
}

impl GameState {
    pub fn start_server(&mut self) -> Option<Entity> {
        if let Ok(server) = self.async_runtime.block_on(run_server()) {
            println!("server running with id {}", server.get_server_id());
            let player_ref = self.spawn_local_player(server.get_server_id());
            self.network_state = Some(NetworkState::ServerConnection(
                server,
                Some(player_ref.clone()),
            ));
            Some(player_ref)
        } else {
            println!("failed to run server");
            None
        }
    }

    pub fn start_client(&mut self, server_iroh_string: String) -> Option<Entity> {
        println!("starting client");
        match self
            .async_runtime
            .block_on(run_client(server_iroh_string.to_string()))
        {
            Ok(client) => {
                println!("client running");
                let player_id = client.get_local_endpoint_id();
                let player_ref = self.spawn_local_player(player_id);
                self.network_state =
                    Some(NetworkState::ClientConnection(client, player_ref.clone()));
                Some(player_ref)
            }
            Err(e) => {
                println!("failed to run client: {:?}", e);
                None
            }
        }
    }

    fn spawn_local_player(&mut self, player_id: PlayerId) -> Entity {
        let player = self.world.spawn((
            player_id.clone(),
            Player {
                position: PlayerPosition { x: 0.0, y: 0.0 },
                width: 50.0,
                height: 50.0,
                is_local: true,
            },
        ));
        self.remote_player_map
            .insert(player_id.clone(), player.clone());
        player
    }

    fn spawn_remote_player(&mut self, player_id: PlayerId) {
        if self.remote_player_map.contains_key(&player_id) {
            return; // Skip if it's the local player or if its already in the map
        }
        let player = self.world.spawn((
            player_id.clone(),
            Player {
                position: PlayerPosition { x: 0.0, y: 0.0 },
                width: 50.0,
                height: 50.0,
                is_local: false,
            },
        ));
        self.remote_player_map
            .insert(player_id.clone(), player.clone());
    }

    fn remove_player(&mut self, player_id: &PlayerId) {
        println!("[client] Player left with ID: {}", player_id);

        // Remove from player map
        self.remote_player_map.remove(player_id.as_str());

        // Remove player from the ECS world
        let mut entities_to_despawn = Vec::new();
        for (entity, id) in self.world.query_mut::<(Entity, &PlayerId)>() {
            if *id == *player_id {
                entities_to_despawn.push(entity);
            }
        }
        for entity in entities_to_despawn {
            self.world.despawn(entity).unwrap();
        }
    }

    fn update_player_with_remote_data(
        &mut self,
        player_id: &PlayerId,
        player_position: &PlayerPosition,
    ) {
        let query = self.world.query_mut::<(&PlayerId, &mut Player)>();
        for (id, player) in query {
            if *id == *player_id {
                player.position = *player_position;
            }
        }
    }

    pub fn poll(&mut self, input: InputData) {
        match &self.network_state {
            Some(NetworkState::ClientConnection(_, _)) => self.poll_client(input),
            Some(NetworkState::ServerConnection(_, _)) => self.poll_server(),
            None => {}
        }
    }

    pub fn poll_client(&mut self, input: InputData) {
        let mut network_state = self.network_state.take();
        if let Some(NetworkState::ClientConnection(client, player_ref)) = &mut network_state {
            // Drain log messages from async tasks
            while let Ok(log_msg) = client.log_receiver.try_recv() {
                println!("{}", log_msg);
            }

            // This is where you can handle any client-related logic
            // For example, you might want to check for incoming messages from the server
            let server_reliable_receiver = client.reliable_server_receiver.clone();
            while let Ok(message) = server_reliable_receiver.try_recv() {
                println!("Received message from server: {:?}", message);
                match message {
                    ReliableServerMessage::Hello { player_id } => {}
                    ReliableServerMessage::PlayersJoined { player_ids } => {
                        for remote_player_id in player_ids {
                            self.spawn_remote_player(remote_player_id);
                        }
                        let query = self.world.query_mut::<(&PlayerId, &mut PlayerPosition)>();
                        let query_vec = query.into_iter().collect::<Vec<_>>();
                        println!("[client] Current players: {:?}", query_vec);
                    }
                    ReliableServerMessage::PlayersLeft { player_ids } => {
                        for player_id in player_ids {
                            self.remove_player(&player_id);
                        }
                    }
                    ReliableServerMessage::Quit => {
                        println!("[client] Server requested to quit");
                    }
                }
            }

            let unreliable_server_receiver = client.unreliable_server_receiver.clone();
            while let Ok(message) = unreliable_server_receiver.try_recv() {
                match message {
                    UnreliableServerMessage::PlayerPosition(remote_player_id, player_data) => {
                        // for now, ignore if updating local player
                        if remote_player_id == client.get_local_endpoint_id() {
                            continue;
                        }
                        self.update_player_with_remote_data(&remote_player_id, &player_data);
                    }
                }
            }

            // Send local player's position to the server
            {
                // Update local player position in the ECS world
                let query = self.world.query_mut::<(&PlayerId, &mut Player)>();
                for (id, player) in query {
                    if *id == client.get_local_endpoint_id() {
                        player.position.x += (input.right as i8 - input.left as i8) as f32 * 5.0;
                        player.position.y += (input.down as i8 - input.up as i8) as f32 * 5.0;
                        // Send position to the server
                        let message = UnreliableClientMessage::PlayerPosition(PlayerPosition {
                            x: player.position.x,
                            y: player.position.y,
                        });
                        if let Err(e) = client.unreliable_client_sender.try_send(message) {
                            println!("Failed to send player position: {:?}", e);
                        }
                        break;
                    }
                }
            }
        }

        self.network_state = network_state;
    }

    pub fn poll_server(&mut self) {
        // This is where you can handle any server-related logic
        // For example, you might want to check for incoming connections or messages
        let mut network_state = self.network_state.take();
        if let Some(NetworkState::ServerConnection(server, player_ref)) = &mut network_state {
            // Drain log messages from server async tasks
            while let Ok(log_msg) = server.log_receiver.try_recv() {
                println!("{}", log_msg);
            }
            let player_ref = player_ref.clone();
            // Handle server logic with the channel_map
            let channel_map = server.channel_map.clone();
            let mut new_player_set = HashSet::<PlayerId>::new();
            let mut leaving_player_set = HashSet::<PlayerId>::new();
            for (player_id, channel) in channel_map.iter() {
                match channel.reliable_receiver.try_recv() {
                    Ok(message) => {
                        //println!("Received message from player {}: {:?}", player_id, message);
                        // Handle the received message
                        match message {
                            ReliableClientMessage::PlayerJoined { player_id } => {
                                println!("Player {} joined", player_id);
                                self.spawn_remote_player(player_id.clone());
                                new_player_set.insert(player_id.clone());
                                // send list of players to player who just joined
                                let mut player_ids: Vec<PlayerId> = channel_map.keys();
                                // Include the host player so clients know about it
                                if player_ref.is_some() {
                                    let host_id = server.get_server_id();
                                    if !player_ids.contains(&host_id) {
                                        player_ids.push(host_id);
                                    }
                                }
                                if let Some(entry) = channel_map.get(&player_id) {
                                    entry
                                        .reliable_sender
                                        .clone()
                                        .try_send(ReliableServerMessage::PlayersJoined {
                                            player_ids,
                                        })
                                        .unwrap();
                                }
                            }
                            ReliableClientMessage::Quit { player_id } => {
                                leaving_player_set.insert(player_id.clone());
                            }
                        }
                    }
                    Err(async_channel::TryRecvError::Empty) => {
                        // No messages available, continue processing
                    }
                    Err(async_channel::TryRecvError::Closed) => {
                        println!("Channel for player {} closed", player_id);
                        leaving_player_set.insert(player_id.clone());
                    }
                }

                match channel.unreliable_receiver.try_recv() {
                    Ok(message) => {
                        // Handle the received message
                        match message {
                            UnreliableClientMessage::PlayerPosition(player_position) => {
                                self.update_player_with_remote_data(&player_id, &player_position);
                            }
                        }
                    }
                    Err(async_channel::TryRecvError::Empty) => {
                        // No messages available, continue processing
                    }
                    Err(async_channel::TryRecvError::Closed) => {
                        println!("Unreliable channel for player {} closed", player_id);
                        leaving_player_set.insert(player_id.clone());
                    }
                }
            }

            // Send messages to clients
            let game_data = self
                .world
                .query::<(&PlayerId, &Player)>()
                .iter()
                .map(|(id, player)| {
                    UnreliableServerMessage::PlayerPosition(id.clone(), player.position)
                })
                .collect::<Vec<UnreliableServerMessage>>();

            // send game data to all players
            for (player_id, message_channels) in channel_map.iter() {
                // Get player position in the world
                let unreliable_server_sender = &message_channels.unreliable_sender;
                // send game data to each player
                for game_data_message in &game_data {
                    // Send player position to the client
                    if let Err(e) = unreliable_server_sender.try_send(game_data_message.clone()) {
                        println!("Failed to send message to player {}: {}", player_id, e);
                    }
                }
            }

            // Send new player messages
            if !new_player_set.is_empty() {
                // send new player message to all players
                let new_player_message = ReliableServerMessage::PlayersJoined {
                    player_ids: new_player_set.iter().cloned().collect::<Vec<PlayerId>>(),
                };
                for (player_id, message_channels) in channel_map.iter() {
                    let reliable_server_sender = &message_channels.reliable_sender;
                    if let Err(e) = reliable_server_sender.try_send(new_player_message.clone()) {
                        println!(
                            "Failed to send new player message to player {}: {}",
                            player_id, e
                        );
                    }
                }
            }

            // Handle leaving players
            if !leaving_player_set.is_empty() {
                for player_id in &leaving_player_set {
                    self.remove_player(player_id);
                }
                let leaving_player_message = ReliableServerMessage::PlayersLeft {
                    player_ids: leaving_player_set
                        .iter()
                        .cloned()
                        .collect::<Vec<PlayerId>>(),
                };
                // shut down channels for leaving players and remove from channel map
                for player_id in leaving_player_set {
                    if let Some(message_channels) = channel_map.get(&player_id) {
                        let _ = message_channels.cancel_sender.send(true);
                    }
                    server.channel_map.remove(&player_id);
                }
                // tell remaining players about leaving players
                for (player_id, message_channels) in channel_map.iter() {
                    let reliable_server_sender = &message_channels.reliable_sender;
                    if let Err(e) = reliable_server_sender.try_send(leaving_player_message.clone())
                    {
                        println!(
                            "Failed to send leaving player message to player {}: {}",
                            player_id, e
                        );
                    }
                }
            }
        }
        self.network_state = network_state;
    }

    pub fn close_client(&mut self) {
        let mut network_state = self.network_state.take();
        if let Some(NetworkState::ClientConnection(client, _)) = &mut network_state {
            // Cancel the client if it is running
            let _ = client.cancel_sender.send(true);
            // Optionally, you can also wait for the client's tasks to finish
            self.async_runtime.block_on(client.join_set.shutdown());
        }
    }

    pub fn close_server(&mut self) {
        let mut network_state = self.network_state.take();
        if let Some(NetworkState::ServerConnection(server, player_ref)) = &mut network_state {
            // Clean up resources if necessary
            for (_player_id, message_channels) in server.channel_map.iter() {
                // shut down the tasks for each player
                let _ = message_channels.cancel_sender.send(true);
            }
            server.channel_map.clear(); // Clear the channel map on exit
        }
    }

    pub fn get_remote_player_amount(&self) -> i32 {
        self.remote_player_map.len() as i32
    }

    pub fn get_local_network_id(&self) -> String {
        if let Some(NetworkState::ClientConnection(client, _)) = &self.network_state {
            return client.get_local_endpoint_id();
        } else if let Some(NetworkState::ServerConnection(server, _)) = &self.network_state {
            return server.get_server_id();
        }
        DEFAULT_PLAYER_ID.to_string()
    }

    pub fn get_local_player_component(&mut self) -> Option<Player> {
        let local_player_id = self.get_local_network_id();
        let query = self.world.query_mut::<(&PlayerId, &Player)>();
        for (id, player) in query {
            if *id == local_player_id {
                return Some(player.clone());
            }
        }
        None
    }

    pub fn get_remote_players(&mut self) -> Vec<(PlayerId, Player)> {
        let mut remote_players = Vec::new();
        let query = self.world.query_mut::<(&PlayerId, &Player)>();
        for (id, player) in query {
            if !player.is_local {
                remote_players.push((id.clone(), player.clone()));
            }
        }
        remote_players
    }
}
