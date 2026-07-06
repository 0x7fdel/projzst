use serde::{Deserialize, Serialize};


pub trait Frame: Deserialize + Serialize {

}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MainFrame {
    /// Frame name
    pub pjz: String,

    /// Package name
    #[serde(default)]
    pub name: Option<String>,

    /// Author name
    #[serde(default)]
    pub auth: Option<String>,

    /// Package format identifier
    #[serde(default)]
    pub fmt: Option<String>,

    /// Format edition
    #[serde(default)]
    pub ed: Option<String>,

    /// Project version
    #[serde(default)]
    pub ver: Option<String>,

    /// Package description
    #[serde(default)]
    pub desc: Option<String>,
}

impl Frame for MainFrame {
    
}

impl MainFrame {
    
}