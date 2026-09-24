use anyhow::{Result, bail};
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::model::{Board, Card, List};

const DEFAULT_BASE: &str = "https://api.trello.com/1";
const CARD_FIELDS: &str = "name,desc,idList,idBoard,pos,closed,labels,due,dueComplete,shortUrl";

#[derive(Clone)]
pub struct TrelloClient {
    http: reqwest::Client,
    base: String,
    key: String,
    token: String,
}

impl TrelloClient {
    pub fn new(key: String, token: String) -> Self {
        // TRELLO_API_BASE points the client at another server, e.g. a local mock.
        let base = std::env::var("TRELLO_API_BASE").unwrap_or_else(|_| DEFAULT_BASE.into());
        Self {
            http: reqwest::Client::new(),
            base,
            key,
            token,
        }
    }

    fn auth(&self) -> [(&str, &str); 2] {
        [("key", &self.key), ("token", &self.token)]
    }

    async fn send<T: DeserializeOwned>(req: reqwest::RequestBuilder) -> Result<T> {
        // Strip the URL from errors: it carries the API token as a query param.
        let resp = req.send().await.map_err(|e| e.without_url())?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Trello API {status}: {}", body.trim());
        }
        Ok(resp.json().await.map_err(|e| e.without_url())?)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let req = self
            .http
            .get(format!("{}{path}", self.base))
            .query(&self.auth())
            .query(query);
        Self::send(req).await
    }

    pub async fn boards(&self) -> Result<Vec<Board>> {
        let mut boards: Vec<Board> = self
            .get(
                "/members/me/boards",
                &[("filter", "open"), ("fields", "name")],
            )
            .await?;
        boards.sort_by_key(|b| b.name.to_lowercase());
        Ok(boards)
    }

    pub async fn lists(&self, board_id: &str) -> Result<Vec<List>> {
        self.get(
            &format!("/boards/{board_id}/lists"),
            &[("filter", "open"), ("fields", "name,pos")],
        )
        .await
    }

    pub async fn cards(&self, board_id: &str) -> Result<Vec<Card>> {
        self.get(
            &format!("/boards/{board_id}/cards"),
            &[("filter", "open"), ("fields", CARD_FIELDS)],
        )
        .await
    }

    pub async fn create_card(&self, list_id: &str, name: &str, pos: f64) -> Result<Card> {
        let req = self
            .http
            .post(format!("{}/cards", self.base))
            .query(&self.auth())
            .form(&[
                ("idList", list_id),
                ("name", name),
                ("pos", &pos.to_string()),
            ]);
        Self::send(req).await
    }

    /// Creates a card from arbitrary fields (`idList` required); `pos` may be `top` / `bottom`.
    pub async fn create_card_with(&self, fields: &[(&str, &str)]) -> Result<Card> {
        let req = self
            .http
            .post(format!("{}/cards", self.base))
            .query(&self.auth())
            .form(fields);
        Self::send(req).await
    }

    /// One card by id or short link.
    pub async fn card(&self, card_id: &str) -> Result<Card> {
        self.get(&format!("/cards/{card_id}"), &[("fields", CARD_FIELDS)])
            .await
    }

    pub async fn add_comment(&self, card_id: &str, text: &str) -> Result<()> {
        let req = self
            .http
            .post(format!("{}/cards/{card_id}/actions/comments", self.base))
            .query(&self.auth())
            .form(&[("text", text)]);
        Self::send::<IgnoredAny>(req).await.map(|_| ())
    }

    pub async fn create_list(&self, board_id: &str, name: &str, pos: f64) -> Result<List> {
        let req = self
            .http
            .post(format!("{}/lists", self.base))
            .query(&self.auth())
            .form(&[
                ("idBoard", board_id),
                ("name", name),
                ("pos", &pos.to_string()),
            ]);
        Self::send(req).await
    }

    /// PUT arbitrary card fields, e.g. `[("name", "x"), ("closed", "true")]`.
    pub async fn update_card(&self, card_id: &str, fields: &[(&str, String)]) -> Result<Card> {
        let req = self
            .http
            .put(format!("{}/cards/{card_id}", self.base))
            .query(&self.auth())
            .form(fields);
        Self::send(req).await
    }

    /// Permanently deletes a card (archiving is `closed=true` via `update_card`).
    pub async fn delete_card(&self, card_id: &str) -> Result<()> {
        let req = self
            .http
            .delete(format!("{}/cards/{card_id}", self.base))
            .query(&self.auth());
        Self::send::<IgnoredAny>(req).await.map(|_| ())
    }

    /// Archives a list. Trello's API has no way to delete a list outright.
    pub async fn archive_list(&self, list_id: &str) -> Result<()> {
        let req = self
            .http
            .put(format!("{}/lists/{list_id}", self.base))
            .query(&self.auth())
            .form(&[("closed", "true")]);
        Self::send::<IgnoredAny>(req).await.map(|_| ())
    }
}
