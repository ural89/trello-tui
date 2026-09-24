use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Board {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct List {
    pub id: String,
    pub name: String,
    pub pos: f64,
}

impl List {
    /// Lists created locally but not yet confirmed by the server.
    pub fn is_pending(&self) -> bool {
        self.id.starts_with("tmp-")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Label {
    #[serde(default)]
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub id_list: String,
    #[serde(default)]
    pub id_board: String,
    pub pos: f64,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub labels: Vec<Label>,
    pub due: Option<String>,
    #[serde(default)]
    pub due_complete: bool,
    #[serde(default)]
    pub short_url: String,
}

impl Card {
    /// Cards created locally but not yet confirmed by the server.
    pub fn is_pending(&self) -> bool {
        self.id.starts_with("tmp-")
    }
}
