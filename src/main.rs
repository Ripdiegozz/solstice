use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use solstice::app::App;
use solstice::config::Config;
use solstice::events::EventStore;
use solstice::gcal::GCalSync;
use solstice::ui;
use std::io;
use std::time::Duration;

fn main() -> Result<()> {
    // Handle CLI subcommands
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "sync" {
        return run_sync();
    }

    // Load configuration
    let config = Config::load()?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(config)?;

    // Main event loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        // Update time
        app.tick();

        // Draw UI
        terminal.draw(|f| {
            ui::draw_ui(f, app);
        })?;

        // Handle events with a timeout for smooth clock updates
        if event::poll(Duration::from_millis(250))? && let Event::Key(key) = event::read()? && key.kind == KeyEventKind::Press {
            // Modal-aware key dispatch
            if app.modal.is_some() {
                app.handle_modal_key(key);
            } else if app.confirm_delete {
                app.handle_delete_confirm(key);
            } else {
                // Normal navigation mode
                match key.code {
                    KeyCode::Char('q') => app.quit(),
                    KeyCode::Esc => {
                        app.status_message = None;
                    }
                    KeyCode::Char('v') => app.toggle_view(),
                    KeyCode::Char('h') | KeyCode::Left => app.navigate_back(),
                    KeyCode::Char('l') | KeyCode::Right => app.navigate_forward(),
                    KeyCode::Char('k') | KeyCode::Up => app.prev_day(),
                    KeyCode::Char('j') | KeyCode::Down => app.next_day(),
                    KeyCode::Char('t') => app.go_to_today(),
                    KeyCode::Char('n') => app.open_create_modal(),
                    KeyCode::Char('e') => {
                        app.open_edit_modal();
                    }
                    KeyCode::Char('d') => {
                        if app.selected_event_index.is_some() {
                            app.confirm_delete = true;
                        } else {
                            app.status_message = Some("No event selected".to_string());
                        }
                    }
                    KeyCode::Tab => app.select_next_event(),
                    KeyCode::BackTab => app.select_prev_event(),
                    _ => {}
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}

/// Run the Google Calendar sync subcommand.
/// Loads config, authenticates, fetches events, and upserts into EventStore.
fn run_sync() -> Result<()> {
    let config = Config::load()?;

    // Validate credentials
    let client_id = match &config.gcal_client_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            println!(
                "❌ Google Calendar credentials not configured.\n\
                 \n\
                 To enable Calendar sync, you need to:\n\
                 \n\
                 1. Go to https://console.cloud.google.com/\n\
                 2. Create a new project (or select an existing one)\n\
                 3. Enable the Google Calendar API\n\
                 4. Create OAuth 2.0 credentials (Desktop application)\n\
                 5. Download the credentials and add to your config:\n\
                 \n\
                 Config file: {}/config.toml\n\
                 \n\
                 Add these fields:\n\
                 gcal_client_id = \"YOUR_CLIENT_ID.apps.googleusercontent.com\"\n\
                 gcal_client_secret = \"YOUR_CLIENT_SECRET\"\n",
                solstice::config::config_dir().display()
            );
            return Ok(());
        }
    };

    let client_secret = match &config.gcal_client_secret {
        Some(secret) if !secret.is_empty() => secret.clone(),
        _ => {
            println!("❌ gcal_client_secret is missing from config. See setup instructions above.");
            return Ok(());
        }
    };

    println!("🔄 Starting Google Calendar sync...\n");

    // Initialize sync client
    let sync = GCalSync::new(&client_id, &client_secret)?;

    // Authenticate (or load/refresh existing token)
    let mut token = sync.load_or_refresh_token()?;

    // Define sync window: now - 30 days to now + 90 days
    let now = chrono::Utc::now();
    let time_min = now - chrono::Duration::days(30);
    let time_max = now + chrono::Duration::days(90);

    println!("📅 Fetching events from {} to {}...",
        time_min.format("%Y-%m-%d"),
        time_max.format("%Y-%m-%d")
    );

    // Fetch events from Google Calendar API (mut for reactive 401 retry)
    let events = sync.fetch_events(&mut token, time_min, time_max, &config.timezone)?;
    println!("📥 Fetched {} events from Google Calendar", events.len());

    // Open event store and sync
    let store = EventStore::open()?;

    // Clear stale gcal events first (full replace strategy)
    let deleted = store.clear_gcal_events()?;
    if deleted > 0 {
        println!("🗑️  Cleared {} stale synced events", deleted);
    }

    // Upsert all fetched events
    let mut upserted = 0;
    for event in &events {
        store.upsert_gcal_event(event)?;
        upserted += 1;
    }

    println!("✅ Sync complete: {} events upserted into local store\n", upserted);
    Ok(())
}
