use clap::{Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SearchModeArg {
    Lexical,
    Hybrid,
    Semantic,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum SearchSortArg {
    Date,
    Relevance,
}

impl From<SearchModeArg> for crate::mxr_core::SearchMode {
    fn from(value: SearchModeArg) -> Self {
        match value {
            SearchModeArg::Lexical => Self::Lexical,
            SearchModeArg::Hybrid => Self::Hybrid,
            SearchModeArg::Semantic => Self::Semantic,
        }
    }
}

impl From<SearchSortArg> for crate::mxr_core::types::SortOrder {
    fn from(value: SearchSortArg) -> Self {
        match value {
            SearchSortArg::Date => crate::mxr_core::types::SortOrder::DateDesc,
            SearchSortArg::Relevance => crate::mxr_core::types::SortOrder::Relevance,
        }
    }
}

#[derive(Subcommand)]
pub enum SemanticAction {
    /// Show semantic status
    Status,
    /// Enable semantic search using the active profile
    Enable,
    /// Disable semantic search
    Disable,
    /// Reindex the active semantic profile
    Reindex,
    /// Manage semantic profiles
    Profile {
        #[command(subcommand)]
        action: Option<SemanticProfileAction>,
    },
}

#[derive(Subcommand)]
pub enum SemanticProfileAction {
    /// List known semantic profiles
    List,
    /// Install a semantic profile without switching to it
    Install {
        #[arg(value_enum)]
        profile: SemanticProfileArg,
    },
    /// Switch to a semantic profile and rebuild its index
    Use {
        #[arg(value_enum)]
        profile: SemanticProfileArg,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SemanticProfileArg {
    #[value(name = "bge-small-en-v1.5")]
    BgeSmallEnV15,
    #[value(name = "multilingual-e5-small")]
    MultilingualE5Small,
    #[value(name = "bge-m3")]
    BgeM3,
}

impl From<SemanticProfileArg> for crate::mxr_core::SemanticProfile {
    fn from(value: SemanticProfileArg) -> Self {
        match value {
            SemanticProfileArg::BgeSmallEnV15 => Self::BgeSmallEnV15,
            SemanticProfileArg::MultilingualE5Small => Self::MultilingualE5Small,
            SemanticProfileArg::BgeM3 => Self::BgeM3,
        }
    }
}
