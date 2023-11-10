use std::borrow::Cow;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result};
use indicatif::ProgressBar;
use rustyline::DefaultEditor;
use tokio_util::sync::CancellationToken;

use crate::openai::OpenAI;
use crate::{Config, CARGO_PKG_NAME};

pub(crate) struct SerMaid {
    editor: DefaultEditor,
    history_file: Option<PathBuf>,

    openai: OpenAI,

    history_questions: Vec<String>,
    history_answers: Vec<Cow<'static, str>>,
}

impl SerMaid {
    pub fn from_config(config: Config) -> Result<Self> {
        let mut editor =
            DefaultEditor::new().wrap_err_with(|| "failed to initialize rustyline editor")?;
        if let Some(history_file) = &config.history_file {
            let _ = editor.load_history(history_file);
        }

        Ok(Self {
            editor,
            history_file: config.history_file,
            openai: OpenAI::new(config.api_token),
            history_questions: Vec::new(),
            history_answers: Vec::new(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            let mut command = String::new();
            for line in self.editor.iter("> ") {
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

            self.editor
                .add_history_entry(command.clone())
                .wrap_err_with(|| {
                    format!("failed to add history entry `{command}` to rustyline editor")
                })?;
            if let Some(history_file) = &self.history_file {
                self.editor.save_history(history_file).wrap_err_with(|| {
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
                },
            };

            let mut args = vec![CARGO_PKG_NAME.to_owned()];
            args.append(&mut split);

            if !self.command_and_continue(args).await {
                return Ok(());
            }
        }
    }

    async fn command_and_continue(&mut self, args: Vec<String>) -> bool {
        let args = match Cli::try_parse_from(args) {
            Ok(args) => args,
            Err(err) => {
                println!("{err}");
                return true;
            },
        };

        match args.command {
            Command::Ask { question } => {
                let question = shell_words::join(question);
                if let Some(answer) =
                    ask_openai(|| self.openai.q_and_a(question.clone(), &[], &[])).await
                {
                    self.history_questions.push(question);
                    self.history_answers.push(answer);
                }
            },
            Command::Continue { question } => {
                let question = shell_words::join(question);
                if let Some(answer) = ask_openai(|| {
                    self.openai.q_and_a(
                        question.clone(),
                        &self.history_questions,
                        &self.history_answers,
                    )
                })
                .await
                {
                    self.history_questions.push(question);
                    self.history_answers.push(answer);
                }
            },
            Command::Translate { raw_text } => {
                ask_openai(|| self.openai.translate(shell_words::join(raw_text))).await;
            },
            Command::Clear => {
                if let Err(err) = self
                    .editor
                    .clear_screen()
                    .wrap_err_with(|| "failed to clear screen")
                {
                    println!("{err:?}");
                };
            },
            Command::Exit => {
                return false;
            },
        }

        true
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
        },
        Err(err) => {
            println!("{err:?}");
            None
        },
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
