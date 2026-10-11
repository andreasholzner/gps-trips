use serde::{Deserialize, Serialize};

/// What happens to one of the uploaded files' tracks or waypoints (US-88):
/// the server proposes, the owner decides (ADR-0029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportAction {
    /// Into the collection as a track or waypoint of its own.
    Add,
    /// Left out: the collection already holds it, or the owner does not want it.
    Skip,
    /// In place of the one it matches, which keeps its identity.
    Replace,
}

impl ImportAction {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportAction::Add => "add",
            ImportAction::Skip => "skip",
            ImportAction::Replace => "replace",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ImportAction::Add => "Add",
            ImportAction::Skip => "Skip",
            ImportAction::Replace => "Replace",
        }
    }

    /// The action a wire value names, if any.
    pub fn parse(value: &str) -> Option<Self> {
        [Self::Add, Self::Skip, Self::Replace]
            .into_iter()
            .find(|action| action.as_str() == value)
    }
}

/// One track or waypoint of the uploaded files, as the review lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewItem {
    /// The file it comes from, an index into
    /// [`CollectionImportReview::files`].
    pub file: usize,
    /// Its place among that file's tracks, or its waypoints.
    pub index: usize,
    /// As the file names it; `None` for an unnamed one.
    pub name: Option<String>,
    /// What it matches, if anything.
    pub matched: Option<ReviewMatch>,
    pub proposed: ImportAction,
    /// The owner's choice where it is one of `choices`, else the proposal.
    pub action: ImportAction,
    pub choices: Vec<ImportAction>,
}

/// What an uploaded track or waypoint matches: one the collection holds, or
/// one an earlier file of the same upload brings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewMatch {
    pub name: Option<String>,
    /// `None` for what the collection holds, else the earlier file's index.
    pub file: Option<usize>,
}

/// What the collection holds and none of the files does: kept unless the
/// owner removes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeldItem {
    pub id: i64,
    pub name: Option<String>,
    pub remove: bool,
}

/// What `POST /api/collection-imports` and its review answer with: the
/// uploaded files, parked until confirmed, and what each of their tracks and
/// waypoints would become (US-88).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionImportReview {
    pub staging_id: i64,
    /// The collection the files are added to; `None` for a new one.
    pub collection_id: Option<i64>,
    /// A new collection's details, from the first file.
    pub suggested_name: String,
    pub suggested_description: String,
    /// The files' names, in the order they are taken.
    pub files: Vec<String>,
    pub replace_all: bool,
    pub tracks: Vec<ReviewItem>,
    pub waypoints: Vec<ReviewItem>,
    pub held_tracks: Vec<HeldItem>,
    pub held_waypoints: Vec<HeldItem>,
    /// What the collection held when this was worked out; a confirmation
    /// carrying another is refused, since the owner decided on something else.
    pub held_digest: String,
}

/// The owner's choice for one uploaded track or waypoint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ItemDecision {
    pub file: usize,
    pub index: usize,
    pub action: ImportAction,
}

/// The owner's decisions so far; whatever is not decided takes the proposal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportDecisions {
    /// Everything the collection holds goes, and the files' contents come in.
    #[serde(default)]
    pub replace_all: bool,
    #[serde(default)]
    pub tracks: Vec<ItemDecision>,
    #[serde(default)]
    pub waypoints: Vec<ItemDecision>,
    /// Held tracks and waypoints none of the files holds, to be removed.
    #[serde(default)]
    pub remove_tracks: Vec<i64>,
    #[serde(default)]
    pub remove_waypoints: Vec<i64>,
}

/// `POST /api/collection-imports/:id/confirm`'s body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfirmCollectionImport {
    pub decisions: ImportDecisions,
    pub held_digest: String,
    /// A new collection's details, as US-86's import takes them; ignored when
    /// adding to an existing one.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub activity_type: Option<String>,
}

/// One of the files a collection was imported or added from, kept as it
/// was uploaded (US-88).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionFile {
    pub id: i64,
    pub file_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us88_an_action_reads_back_from_its_wire_value() {
        for action in [ImportAction::Add, ImportAction::Skip, ImportAction::Replace] {
            assert_eq!(ImportAction::parse(action.as_str()), Some(action));
            assert_eq!(
                serde_json::to_string(&action).unwrap(),
                format!("\"{}\"", action.as_str())
            );
        }
        assert_eq!(ImportAction::parse("keep"), None);
    }

    #[test]
    fn us88_decisions_left_out_are_none() {
        let decisions: ImportDecisions = serde_json::from_str("{}").unwrap();

        assert_eq!(decisions, ImportDecisions::default());
    }
}
