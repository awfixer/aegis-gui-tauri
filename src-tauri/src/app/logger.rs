use std::{
    fs::File,
    io::{self, Write},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager};
use tracing::{field::Field, Event, Subscriber};
use tracing_appender::rolling;
use tracing_subscriber::{
    fmt,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    Layer,
};
/*
Handling of the logging. Sends the logs to the frontend,
to the file and to the terminal(stdout).
*/
#[derive(Debug)]

pub struct FileLogger {
    file: Arc<Mutex<File>>,
}

impl FileLogger {
    #[allow(dead_code)]
    pub fn new(path: &str) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(FileLogger {
            file: Arc::new(Mutex::new(file)),
        })
    }
    #[allow(dead_code)]
    pub fn write(&self, buf: &[u8]) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();
        file.write_all(buf)?;
        Ok(())
    }

    pub fn writeln(&self, args: std::fmt::Arguments) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();
        writeln!(file, "{}", args)
    }
}

pub struct FileLoggerLayer {
    logger: Arc<FileLogger>,
}

impl FileLoggerLayer {
    #[allow(dead_code)]
    pub fn new(logger: Arc<FileLogger>) -> Self {
        FileLoggerLayer { logger }
    }
}

impl<S: Subscriber> Layer<S> for FileLoggerLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let meta = event.metadata();
        if let Ok(_) = self
            .logger
            .writeln(format_args!("{}: {:#?}", meta.target(), event))
        {
            // Handle write error or ignore
        }
    }
}
#[allow(dead_code)]
pub struct Logger {}

impl Logger {
    // initialize a global subscriber
    #[allow(dead_code)]
    pub fn start(app_handle: AppHandle) {
        let file_appender = rolling::daily("./logs", "prefix.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        // Set up the subscriber to write to both the terminal and the file
        tracing_subscriber::registry()
            .with(
                fmt::layer().with_writer(std::io::stdout), // For terminal output
            )
            .with(
                fmt::layer().with_writer(non_blocking), // For file output
            )
            .with(TauriLogEmitter {
                app_handle: app_handle.clone(),
            }) // Sending to the frontend
            .init();
    }
}

pub struct TauriLogEmitter {
    pub app_handle: AppHandle,
}

impl TauriLogEmitter {
    #[allow(dead_code)]
    pub fn new(app_handle: AppHandle) -> Self {
        TauriLogEmitter { app_handle }
    }
}

impl<S> Layer<S> for TauriLogEmitter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut message = String::new();
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        message.push_str(&format!(
            "{timestamp} [{}]",
            event.metadata().level().as_str().to_uppercase()
        ));
        event.record(&mut |field: &Field, value: &dyn std::fmt::Debug| {
            if !message.is_empty() {
                message.push_str(", ");
            }
            message.push_str(&format!("{}: {:?}", field.name(), value));
        });

        // emit log event to the frontend
        self.app_handle
            .emit_all("log", message)
            .expect("Failed to emit log event");
    }
}

use log::{Level, LevelFilter, Metadata, Record};

pub struct TauriLogger {
    app_handle: AppHandle,
    file: Mutex<File>,
}

impl TauriLogger {
    pub fn new(app_handle: AppHandle, log_file_path: &str) -> Self {
        let file = File::create(log_file_path).expect("Failed to create log file");
        TauriLogger {
            app_handle,
            file: Mutex::new(file),
        }
    }
}

impl log::Log for TauriLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
            let message = format!(
                "{timestamp} [{}] - {}: {}",
                record.level(),
                record.target(),
                record.args()
            );

            // Write to terminal
            println!("{}", message);

            // Write to file
            if let Ok(mut file) = self.file.lock() {
                writeln!(file, "{}", message).expect("Failed to write to log file");
            }

            // Emit to Tauri frontend
            self.app_handle
                .emit_all("log", &message)
                .expect("Failed to emit log event");
        }
    }

    fn flush(&self) {}
}

pub fn setup_logging(app_handle: AppHandle) {
    let logger = TauriLogger::new(app_handle, "./logs/app/prefix.log");
    log::set_boxed_logger(Box::new(logger)).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Info);
}
