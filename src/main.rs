use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use solstice::app::App;
use solstice::config::Config;
use solstice::events::EventStore;
use solstice::gcal::GCalSync;
use solstice::ui;
use std::io;
use std::time::Duration;

/// RAII guard that runs a cleanup closure on drop.
struct TerminalGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> TerminalGuard<F> {
    fn new(cleanup: F) -> Self {
        Self { cleanup: Some(cleanup) }
    }
}

impl<F: FnOnce()> Drop for TerminalGuard<F> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

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
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Panic hook to ensure terminal is restored on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture, Show);
        original_hook(info);
    }));

    // Create app state
    let mut app = App::new(config)?;

    // RAII guard for clean exit
    let guard = TerminalGuard::new(|| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture, Show);
    });

    // Main event loop
    let result = run_app(&mut terminal, &mut app);

    // Explicitly drop the guard to run cleanup
    drop(guard);

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
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    // Modal-aware key dispatch
                    if app.modal.is_some() {
                        app.handle_modal_key(key);
                    } else if app.confirm_delete {
                        app.handle_delete_confirm(key);
                    } else {
                        // Numbered panel switching (absorbed when modal/confirm open)
                        match key.code {
                            KeyCode::Char('1') => app.focus_panel(1),
                            KeyCode::Char('2') => app.focus_panel(2),
                            KeyCode::Char('3') => app.focus_panel(3),
                            _ => app.handle_normal_key(key),
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    if app.modal.is_some() || app.confirm_delete {
                        app.handle_mouse(mouse, None);
                    } else {
                        let size = terminal.size()?;
                        let rects = ui::compute_layout(Rect::new(0, 0, size.width, size.height));
                        app.handle_mouse(mouse, Some(&rects));
                    }
                }
                _ => {}
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
