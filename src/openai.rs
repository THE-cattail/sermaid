use std::borrow::Cow;

use color_eyre::eyre::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const OPENAI_ENDPOINT_PREFIX: &str = "https://api.openai.com/v1";
const MODEL: &str = "gpt-4-1106-preview";

pub struct OpenAI {
    api_token: String,
    cli: Client,
}

impl OpenAI {
    pub fn new(api_token: String) -> Self {
        Self {
            api_token,
            cli: Client::new(),
        }
    }

    pub async fn q_and_a<S>(
        &self,
        question: S,
        history_questions: &[String],
        history_answers: &[Cow<'static, str>],
    ) -> Result<Cow<'static, str>>
    where
        S: Into<Cow<'static, str>>,
    {
        let mut req = Request::new().with_temperature(0).append(Message::new(
            "回答问题，语言简练不复读不举例子不做额外解释禁止胡编",
            Role::System,
        ));

        let mut history_questions_iter = history_questions.iter();
        let mut history_answers_iter = history_answers.iter();
        loop {
            let history_question = history_questions_iter.next();
            let history_answer = history_answers_iter.next();

            if history_question.is_none() && history_answer.is_none() {
                break;
            }

            if let Some(history_question) = history_question {
                req = req.append(Message::new(history_question.to_owned(), Role::User));
            }

            if let Some(history_answer) = history_answer {
                req = req.append(Message::new(history_answer.clone(), Role::Assistant));
            }
        }

        req = req.append(Message::new(question, Role::User));

        self.chat_completions(&req).await
    }

    pub async fn translate<S>(&self, raw_text: S) -> Result<Cow<'static, str>>
    where
        S: Into<Cow<'static, str>>,
    {
        let req = Request::new()
            .with_temperature(0)
            .append(Message::new(
                "翻成中文，用户输入中文则翻成英语",
                Role::System,
            ))
            .append(Message::new(raw_text, Role::User));

        self.chat_completions(&req).await
    }

    pub async fn commit<S>(&self, raw_text: S) -> Result<Cow<'static, str>>
    where
        S: Into<Cow<'static, str>>,
    {
        let req = Request::new()
            .with_temperature(0)
            .append(Message::new(
                "根据摘要用英文写符合 conventional commits 规范的 commit 文本",
                Role::System,
            ))
            .append(Message::new(raw_text, Role::User));

        self.chat_completions(&req).await
    }

    async fn chat_completions(&self, req: &Request) -> Result<Cow<'static, str>> {
        let url = format!("{OPENAI_ENDPOINT_PREFIX}/chat/completions");

        let req = self
            .cli
            .post(url)
            .bearer_auth(&self.api_token)
            .json(req)
            .build()?;
        tracing::debug!(
            "chat_completions req = {:?}",
            String::from_utf8(req.body().unwrap().as_bytes().unwrap().to_vec()).unwrap()
        );

        let resp = self.cli.execute(req).await?.json::<Response>().await?;

        let mut choices = if let Some(choices) = resp.choices {
            choices
        } else {
            let message = if let Some(error) = resp.error {
                error.message
            } else {
                String::new()
            };

            color_eyre::eyre::bail!("failed to request chat completions{message}",);
        };

        Ok(choices
            .pop()
            .ok_or_else(|| color_eyre::eyre::eyre!("empty choices"))?
            .message
            .content)
    }
}

#[derive(Debug, Deserialize)]
struct Error {
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    content: Cow<'static, str>,
    role: Role,
}

impl Message {
    fn new<S>(content: S, role: Role) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        Self {
            content: content.into(),
            role,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Serialize)]
struct Request {
    messages: Vec<Message>,

    model: &'static str,

    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<u8>,
}

impl Request {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
            model: MODEL,
            temperature: None,
        }
    }

    fn append(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    fn with_temperature(mut self, temperature: u8) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

#[derive(Debug, Deserialize)]
struct Response {
    choices: Option<Vec<Choice>>,

    error: Option<Error>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}
