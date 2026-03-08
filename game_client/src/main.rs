use clap::Parser;
use egor::{
    app::{
        App, FrameContext,
        egui::{self, Popup, Window},
    },
    math::{Vec2, vec2},
    render::Color,
};
use std::sync::{Arc, Mutex};

use crate::game_state::{GameState, InputData};

mod game_state;

fn render_game(gfx: &mut egor::render::Graphics, game_state: Arc<Mutex<GameState>>) {
    let mut game_state = game_state.lock().unwrap();
    let local_player = game_state.get_local_player_component();
    if let Some(local_player) = local_player {
        gfx.rect()
            .at(vec2(local_player.position.x, local_player.position.y))
            .size(Vec2::new(local_player.width, local_player.height))
            .color(Color::RED);
    }

    // Render remote players
    let remote_players = game_state.get_remote_players();
    for (client_id, player) in remote_players {
        gfx.rect()
            .at(vec2(player.position.x, player.position.y))
            .size(Vec2::new(player.width, player.height))
            .color(Color::BLUE);
    }
}

fn run_ui_callbacks(
    egui_context: &egui::Context,
    game_state: Arc<Mutex<GameState>>,
    server_address: &mut String,
) {
    Window::new("Debug").show(egui_context, |ui: &mut egui::Ui| {
        let mut game_state = game_state.lock().unwrap();
        if let Some(local_player) = game_state.get_local_player_component() {
            ui.label(format!(
                "Local player position: ({:.2}, {:.2})",
                local_player.position.x, local_player.position.y
            ));
        } else {
            ui.label("Local player not found");
        }
        ui.label("Enter server address to connect:");
        ui.text_edit_singleline(server_address);
        if ui.button("Start Client").clicked() {
            println!("connecting to {}", server_address);
            let address = server_address.clone();
            println!("{:?}", game_state.start_client(address));
        }
        if ui.button("Start Server").clicked() {
            println!("starting server");
            game_state.start_server();
        }
    });
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Optional server ID to connect to
    server_id: Option<String>,
}

/// The main entrypoint.
pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut server_address = cli.server_id.unwrap_or_else(|| "Put server ip here".to_string());
    let game_state = Arc::new(Mutex::new(GameState::default()));

    App::new().title("Hot Reload Demo").run(
        move |FrameContext {
                  events,
                  app,
                  gfx,
                  input,
                  timer,
                  egui_ctx,
              }| {
            let delta = timer.delta;
            let input = InputData {
                up: input.key_held(egor::input::KeyCode::ArrowUp),
                down: input.key_held(egor::input::KeyCode::ArrowDown),
                left: input.key_held(egor::input::KeyCode::ArrowLeft),
                right: input.key_held(egor::input::KeyCode::ArrowRight),
            };

            {
                let mut game_state = game_state.lock().unwrap();
                game_state.poll(input);
            }

            render_game(gfx, game_state.clone());
            run_ui_callbacks(egui_ctx, game_state.clone(), &mut server_address);
        },
    );
    Ok(())
}
