mod command;
mod openai;

use std::{borrow::Cow, future::Future, path::PathBuf, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result};
use food::bin::ConfigPathGetter;
use indicatif::ProgressBar;
use openai::OpenAI;
use rustyline::DefaultEditor;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    /// Specify configuration file
    #[arg(short, long, value_name = "FILE", default_value = "./config.toml")]
    pub config: PathBuf,
}

impl ConfigPathGetter for Args {
    fn config_path(&self) -> &std::path::Path {
        &self.config
    }
}

#[derive(Deserialize)]
struct Config {
    api_token: String,
    pub history_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    food::log::init(CARGO_PKG_NAME).wrap_err_with(|| "failed to initialize food::log")?;

    let (_, config): (Args, Config) = food::bin::get_args_and_config()
        .wrap_err_with(|| "failed to initialize arguments and config")?;

    let openai = OpenAI::new(config.api_token);

    let mut editor =
        DefaultEditor::new().wrap_err_with(|| "failed to initialize rustyline editor")?;
    if let Some(history_file) = &config.history_file {
        let _ = editor.load_history(history_file);
    }

    let mut history_questions = Vec::new();
    let mut history_answers = Vec::new();

    loop {
        let mut command = String::new();
        for line in editor.iter("> ") {
            let mut line = line.wrap_err_with(|| "failed to get rustyline editor line")?;

            line = line.trim().to_owned();
            let eoln = !line.ends_with('\\');

            if !eoln {
                line.pop();
                line.push('\n');
            }

            command = format!("{command}{line}");

            if eoln {
                break;
            }
        }

        editor
            .add_history_entry(command.clone())
            .wrap_err_with(|| {
                format!("failed to add history entry `{command}` to rustyline editor")
            })?;
        if let Some(history_file) = &config.history_file {
            editor.save_history(history_file).wrap_err_with(|| {
                format!(
                    "failed to save history to file `{}`",
                    history_file.display()
                )
            })?;
        }

        let mut split = match shell_words::split(&command)
            .wrap_err_with(|| format!("failed to split command `{command}`"))
        {
            Ok(split) => split,
            Err(err) => {
                println!("{err:?}");
                continue;
            }
        };

        let mut args = vec![CARGO_PKG_NAME.to_owned()];
        args.append(&mut split);

        let args = match Cli::try_parse_from(args) {
            Ok(args) => args,
            Err(err) => {
                println!("{err}");
                continue;
            }
        };

        match args.command {
            Command::Ask { question } => {
                let question = shell_words::join(question);
                if let Some(answer) =
                    ask_openai(|| openai.q_and_a(question.clone(), &[], &[])).await
                {
                    history_questions.push(question);
                    history_answers.push(answer);
                }
            }
            Command::Continue { question } => {
                let question = shell_words::join(question);
                if let Some(answer) = ask_openai(|| {
                    openai.q_and_a(question.clone(), &history_questions, &history_answers)
                })
                .await
                {
                    history_questions.push(question);
                    history_answers.push(answer);
                }
            }
            Command::Translate { raw_text } => {
                ask_openai(|| openai.translate(shell_words::join(raw_text))).await;
            }
            Command::Clear => {
                editor.clear_screen()?;
            }
            Command::Exit => {
                break Ok(());
            }
        }
        continue;
    }
}

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    /// Ask a simple question to OpenAI API and get an answer
    #[clap(alias = "q")]
    Ask { question: Vec<String> },
    /// Continue asking conversation
    #[clap(alias = "c")]
    Continue { question: Vec<String> },
    /// Ask OpenAI API to translate to Chinese, or translate Chinese to English
    #[clap(alias = "tr")]
    Translate { raw_text: Vec<String> },
    /// Clear screen
    Clear,
    /// Exit the program
    Exit,
}

async fn ask_openai<F, Fut>(f: F) -> Option<Cow<'static, str>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Cow<'static, str>>>,
{
    let spinner = Spinner::new();
    spinner.start();

    let res = f()
        .await
        .wrap_err_with(|| "failed to get response from openai");
    spinner.stop();
    match res {
        Ok(content) => {
            println!("{content}");
            Some(content)
        }
        Err(err) => {
            println!("{err:?}");
            None
        }
    }
}

struct Spinner {
    bar: Arc<ProgressBar>,
    cancellation_token: CancellationToken,
}

impl Spinner {
    fn new() -> Self {
        Self {
            bar: Arc::new(ProgressBar::new_spinner().with_message("Waiting for response...")),
            cancellation_token: CancellationToken::new(),
        }
    }

    fn start(&self) {
        let bar = self.bar.clone();
        let cancellation_token = self.cancellation_token.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            loop {
                tokio::select! {
                    _ = tick.tick() => bar.tick(),
                    _ = cancellation_token.cancelled() => break,
                };
            }
        });
    }

    fn stop(&self) {
        self.cancellation_token.cancel();
        self.bar.finish_and_clear();
    }
}
