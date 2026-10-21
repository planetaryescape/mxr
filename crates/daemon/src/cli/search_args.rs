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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SearchGroupByArg {
    From,
    List,
    Category,
}

impl From<SearchGroupByArg> for mxr_protocol::SearchAggregationGroupBy {
    fn from(value: SearchGroupByArg) -> Self {
        match value {
            SearchGroupByArg::From => Self::From,
            SearchGroupByArg::List => Self::List,
            SearchGroupByArg::Category => Self::Category,
        }
    }
}

impl From<SearchModeArg> for mxr_core::SearchMode {
    fn from(value: SearchModeArg) -> Self {
        match value {
            SearchModeArg::Lexical => Self::Lexical,
            SearchModeArg::Hybrid => Self::Hybrid,
            SearchModeArg::Semantic => Self::Semantic,
        }
    }
}

impl From<SearchSortArg> for mxr_core::types::SortOrder {
    fn from(value: SearchSortArg) -> Self {
        match value {
            SearchSortArg::Date => Self::DateDesc,
            SearchSortArg::Relevance => Self::Relevance,
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
    Reindex {
        /// Re-embed every message, including ones whose stored vectors already
        /// look current. Use this to recover a corrupt or half-migrated index.
        #[arg(long)]
        force: bool,
    },
    /// Manage semantic profiles
    Profile {
        #[command(subcommand)]
        action: Option<SemanticProfileAction>,
    },
}

#[derive(Subcommand)]
pub enum LlmAction {
    /// Show each AI feature's model, the providers it goes through and their health
    Status,
    /// Manage the Anthropic, OpenAI and OpenRouter API keys used for hosted models
    Key {
        #[command(subcommand)]
        action: LlmKeyAction,
    },
}

#[derive(Subcommand)]
pub enum LlmKeyAction {
    /// Store a key read from stdin (never from an argument, so it stays out
    /// of shell history and process listings)
    Set {
        #[arg(value_enum)]
        provider: LlmProviderArg,
        /// Check the key and show what would be stored, without storing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Show which providers have a key and where it comes from (never the key)
    Status,
    /// Delete a stored key
    Remove {
        #[arg(value_enum)]
        provider: LlmProviderArg,
        /// Show what would be removed, without removing it
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LlmProviderArg {
    Anthropic,
    Openai,
    Openrouter,
}

impl From<LlmProviderArg> for mxr_protocol::LlmProviderData {
    fn from(value: LlmProviderArg) -> Self {
        match value {
            LlmProviderArg::Anthropic => Self::Anthropic,
            LlmProviderArg::Openai => Self::Openai,
            LlmProviderArg::Openrouter => Self::Openrouter,
        }
    }
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

impl From<SemanticProfileArg> for mxr_core::SemanticProfile {
    fn from(value: SemanticProfileArg) -> Self {
        match value {
            SemanticProfileArg::BgeSmallEnV15 => Self::BgeSmallEnV15,
            SemanticProfileArg::MultilingualE5Small => Self::MultilingualE5Small,
            SemanticProfileArg::BgeM3 => Self::BgeM3,
        }
    }
}
