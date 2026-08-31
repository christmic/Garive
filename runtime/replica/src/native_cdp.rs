//! Runtime mapping from private CDP AX values into T2 semantic observations.

use std::collections::{BTreeMap, BTreeSet};

use garive_browser_cdp::{CdpAxNode, CdpAxTree};
use sha2::{Digest, Sha256};

use crate::{
    NativeNodeRef, NativeObservationBounds, NativeObservationV1, NativeProtocolError,
    NativeSemanticNode, NativeSensitivity, NativeSnapshotId, NativeTarget,
};

/// Immutable Runtime context applied to one raw CDP Accessibility tree.
pub struct CdpObservationContext {
    /// Exact admitted Browser target.
    pub target: NativeTarget,
    /// New Runtime snapshot identity.
    pub snapshot_id: NativeSnapshotId,
    /// Exact Browser target revision.
    pub target_revision: String,
    /// Exact semantic collection bounds.
    pub bounds: NativeObservationBounds,
}

/// Observation plus the private adapter identity binding retained by Runtime.
pub struct MappedCdpObservation {
    /// Redacted bounded observation admissible to Core.
    pub observation: NativeObservationV1,
    /// Private snapshot-local CDP action binding.
    pub binding: CdpSnapshotBindingV1,
}

/// Private binding from one committed snapshot to adapter node mechanics.
pub struct CdpSnapshotBindingV1 {
    target: NativeTarget,
    snapshot_id: NativeSnapshotId,
    target_revision: String,
    focused_node: Option<NativeNodeRef>,
    nodes: BTreeMap<NativeNodeRef, CdpBoundNode>,
}

struct CdpBoundNode {
    backend_dom_node_id: Option<u64>,
    frame_id: Option<String>,
    actions: BTreeSet<String>,
}

/// Exact adapter-private mechanics selected for one semantic element action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpElementTarget {
    /// Current backend DOM node identity, never exposed to Core.
    pub backend_dom_node_id: u64,
    /// Optional owning frame identity.
    pub frame_id: Option<String>,
}

impl CdpSnapshotBindingV1 {
    /// Revalidates the exact page snapshot without selecting an element.
    pub fn validate_page(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
    ) -> Result<(), NativeProtocolError> {
        if target != &self.target {
            return Err(NativeProtocolError::TargetNotAdmitted);
        }
        if expected_snapshot_id != &self.snapshot_id || target_revision != self.target_revision {
            return Err(NativeProtocolError::SnapshotStale);
        }
        Ok(())
    }

    /// Resolves the exact focused semantic node retained by this snapshot.
    pub fn resolve_focus(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
    ) -> Result<CdpElementTarget, NativeProtocolError> {
        self.validate_page(target, expected_snapshot_id, target_revision)?;
        let node = self
            .focused_node
            .as_ref()
            .and_then(|node| self.nodes.get(node))
            .ok_or(NativeProtocolError::FocusChanged)?;
        Ok(CdpElementTarget {
            backend_dom_node_id: node
                .backend_dom_node_id
                .ok_or(NativeProtocolError::FocusChanged)?,
            frame_id: node.frame_id.clone(),
        })
    }

    /// Resolves one click only under exact target, snapshot, revision and action support.
    pub fn resolve_click(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
        node_ref: &NativeNodeRef,
    ) -> Result<CdpElementTarget, NativeProtocolError> {
        self.resolve_element(
            target,
            expected_snapshot_id,
            target_revision,
            node_ref,
            "click",
        )
    }

    /// Resolves text insertion only under exact binding and declared action support.
    pub fn resolve_type_text(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
        node_ref: &NativeNodeRef,
    ) -> Result<CdpElementTarget, NativeProtocolError> {
        self.resolve_element(
            target,
            expected_snapshot_id,
            target_revision,
            node_ref,
            "type_text",
        )
    }

    /// Resolves clear only under exact binding and declared action support.
    pub fn resolve_clear(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
        node_ref: &NativeNodeRef,
    ) -> Result<CdpElementTarget, NativeProtocolError> {
        self.resolve_element(
            target,
            expected_snapshot_id,
            target_revision,
            node_ref,
            "clear",
        )
    }

    /// Resolves native option selection under exact binding and declared support.
    pub fn resolve_select_option(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
        node_ref: &NativeNodeRef,
    ) -> Result<CdpElementTarget, NativeProtocolError> {
        self.resolve_element(
            target,
            expected_snapshot_id,
            target_revision,
            node_ref,
            "select_option",
        )
    }

    fn resolve_element(
        &self,
        target: &NativeTarget,
        expected_snapshot_id: &NativeSnapshotId,
        target_revision: &str,
        node_ref: &NativeNodeRef,
        action: &str,
    ) -> Result<CdpElementTarget, NativeProtocolError> {
        self.validate_page(target, expected_snapshot_id, target_revision)?;
        let node = self
            .nodes
            .get(node_ref)
            .ok_or(NativeProtocolError::NodeStale)?;
        if !node.actions.contains(action) {
            return Err(NativeProtocolError::ActionUnsupported);
        }
        Ok(CdpElementTarget {
            backend_dom_node_id: node
                .backend_dom_node_id
                .ok_or(NativeProtocolError::ActionUnsupported)?,
            frame_id: node.frame_id.clone(),
        })
    }
}

/// Maps raw adapter-private AX identities into a bounded redacted Runtime observation.
pub fn map_cdp_ax_tree(
    context: CdpObservationContext,
    tree: &CdpAxTree,
) -> Result<NativeObservationV1, NativeProtocolError> {
    Ok(map_cdp_ax_tree_with_binding(context, tree)?.observation)
}

/// Maps one AX tree and returns the separate private action binding.
pub fn map_cdp_ax_tree_with_binding(
    context: CdpObservationContext,
    tree: &CdpAxTree,
) -> Result<MappedCdpObservation, NativeProtocolError> {
    if !matches!(context.target, NativeTarget::Browser { .. })
        || tree.nodes.len() > context.bounds.max_nodes as usize
    {
        return Err(NativeProtocolError::InvalidBinding);
    }
    let by_id = tree
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != tree.nodes.len() {
        return Err(NativeProtocolError::ReceiptInvalid);
    }
    let visible = tree
        .nodes
        .iter()
        .filter(|node| !node.ignored)
        .collect::<Vec<_>>();
    let mut semantic_parent = BTreeMap::<&str, Option<&str>>::new();
    for node in &visible {
        semantic_parent.insert(node.node_id.as_str(), nearest_visible_parent(node, &by_id)?);
    }
    let mut ordered = Vec::with_capacity(visible.len());
    let mut emitted = BTreeSet::new();
    while ordered.len() < visible.len() {
        let before = ordered.len();
        for node in &visible {
            if emitted.contains(node.node_id.as_str()) {
                continue;
            }
            let parent = semantic_parent
                .get(node.node_id.as_str())
                .ok_or(NativeProtocolError::ReceiptInvalid)?;
            if parent.is_none_or(|parent| emitted.contains(parent)) {
                emitted.insert(node.node_id.as_str());
                ordered.push(*node);
            }
        }
        if ordered.len() == before {
            return Err(NativeProtocolError::ReceiptInvalid);
        }
    }
    let references = visible
        .iter()
        .map(|node| {
            Ok((
                node.node_id.as_str(),
                node_reference(&context.snapshot_id, &node.node_id)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, NativeProtocolError>>()?;
    let focused = visible
        .iter()
        .filter(|node| {
            node.properties.iter().any(|property| {
                property.name.eq_ignore_ascii_case("focused") && property_truthy(&property.value)
            })
        })
        .collect::<Vec<_>>();
    let deepest_focus = focused
        .iter()
        .filter(|candidate| {
            !focused.iter().any(|other| {
                candidate.node_id != other.node_id
                    && semantic_ancestor(
                        candidate.node_id.as_str(),
                        other.node_id.as_str(),
                        &semantic_parent,
                    )
            })
        })
        .collect::<Vec<_>>();
    if deepest_focus.len() > 1 {
        return Err(NativeProtocolError::ReceiptInvalid);
    }
    let focused_node = deepest_focus
        .first()
        .map(|node| references[node.node_id.as_str()].clone());
    let mut redacted_field_count = 0_u32;
    let nodes = ordered
        .into_iter()
        .map(|node| {
            let protected = protected(node);
            let name = if protected {
                redacted_field_count =
                    redacted_field_count.saturating_add(u32::from(node.name.is_some()));
                node.name.as_ref().map(|_| "[redacted]".into())
            } else {
                node.name.clone()
            };
            let value_summary = if protected {
                redacted_field_count =
                    redacted_field_count.saturating_add(u32::from(node.value_summary.is_some()));
                node.value_summary.as_ref().map(|_| "[redacted]".into())
            } else {
                node.value_summary.clone()
            };
            Ok(NativeSemanticNode {
                node_ref: references[node.node_id.as_str()].clone(),
                parent_ref: semantic_parent[node.node_id.as_str()]
                    .map(|parent| references[parent].clone()),
                role: normalized_token(node.role.as_deref().unwrap_or("generic"))?,
                name,
                value_summary,
                states: states(node)?,
                actions: actions(node)?,
                sensitivity: if protected {
                    NativeSensitivity::Redacted
                } else {
                    NativeSensitivity::Private
                },
            })
        })
        .collect::<Result<Vec<_>, NativeProtocolError>>()?;
    let binding = CdpSnapshotBindingV1 {
        target: context.target.clone(),
        snapshot_id: context.snapshot_id.clone(),
        target_revision: context.target_revision.clone(),
        focused_node: focused_node.clone(),
        nodes: visible
            .iter()
            .map(|node| {
                Ok((
                    references[node.node_id.as_str()].clone(),
                    CdpBoundNode {
                        backend_dom_node_id: node.backend_dom_node_id,
                        frame_id: node.frame_id.clone(),
                        actions: actions(node)?.into_iter().collect(),
                    },
                ))
            })
            .collect::<Result<_, NativeProtocolError>>()?,
    };
    let observation = NativeObservationV1 {
        target: context.target,
        snapshot_id: context.snapshot_id,
        target_revision: context.target_revision,
        nodes,
        focused_node,
        screenshot_reference: None,
        redacted_field_count,
        bounds: context.bounds,
    };
    observation.validate()?;
    Ok(MappedCdpObservation {
        observation,
        binding,
    })
}

fn nearest_visible_parent<'a>(
    node: &'a CdpAxNode,
    by_id: &BTreeMap<&'a str, &'a CdpAxNode>,
) -> Result<Option<&'a str>, NativeProtocolError> {
    let mut current = node.parent_id.as_deref();
    let mut visited = BTreeSet::new();
    while let Some(identity) = current {
        if !visited.insert(identity) {
            return Err(NativeProtocolError::ReceiptInvalid);
        }
        let parent = by_id
            .get(identity)
            .ok_or(NativeProtocolError::ReceiptInvalid)?;
        if !parent.ignored {
            return Ok(Some(parent.node_id.as_str()));
        }
        current = parent.parent_id.as_deref();
    }
    Ok(None)
}

fn semantic_ancestor(
    candidate: &str,
    descendant: &str,
    semantic_parent: &BTreeMap<&str, Option<&str>>,
) -> bool {
    let mut current = semantic_parent.get(descendant).copied().flatten();
    while let Some(parent) = current {
        if parent == candidate {
            return true;
        }
        current = semantic_parent.get(parent).copied().flatten();
    }
    false
}

fn node_reference(
    snapshot_id: &NativeSnapshotId,
    adapter_id: &str,
) -> Result<NativeNodeRef, NativeProtocolError> {
    NativeNodeRef::new(format!(
        "node-{:x}",
        Sha256::digest(format!("{}\0{adapter_id}", snapshot_id.as_str()).as_bytes())
    ))
}

fn protected(node: &CdpAxNode) -> bool {
    let role = node
        .role
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        role.as_str(),
        "password" | "securetextfield" | "secure_text_field"
    ) || node.properties.iter().any(|property| {
        matches!(
            property.name.to_ascii_lowercase().as_str(),
            "protected" | "password" | "secure"
        )
    })
}

fn states(node: &CdpAxNode) -> Result<Vec<String>, NativeProtocolError> {
    let mut values = node
        .properties
        .iter()
        .filter(|property| property_truthy(&property.value))
        .map(|property| normalized_token(&property.name))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn actions(node: &CdpAxNode) -> Result<Vec<String>, NativeProtocolError> {
    let role = normalized_token(node.role.as_deref().unwrap_or("generic"))?;
    let values: &[&str] = match role.as_str() {
        "button" | "link" | "menu_item" | "checkbox" | "radio" => &["click"],
        "textbox" | "text_field" | "searchbox" => &["clear", "type_text"],
        "combobox" | "listbox" => &["select_option"],
        _ => &[],
    };
    Ok(values.iter().map(|value| (*value).into()).collect())
}

fn property_truthy(value: &serde_json::Value) -> bool {
    value.get("value").is_some_and(|value| match value {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::String(value) => !value.is_empty() && value != "false",
        serde_json::Value::Number(value) => value.as_i64() != Some(0),
        _ => false,
    })
}

fn normalized_token(value: &str) -> Result<String, NativeProtocolError> {
    let mut output = String::new();
    let mut previous_lower = false;
    for character in value.chars() {
        if character.is_ascii_uppercase() {
            if previous_lower && !output.ends_with('_') {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_lower = false;
        } else if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_lower = character.is_ascii_lowercase() || character.is_ascii_digit();
        } else if !output.is_empty() && !output.ends_with('_') {
            output.push('_');
            previous_lower = false;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() || output.len() > 128 {
        Err(NativeProtocolError::ReceiptInvalid)
    } else {
        Ok(output)
    }
}
