use std::collections::BTreeSet;

use garive_core::{
    derive_context, CandidateKind, ContextCandidate, ContextItem, ContextPurpose, ContextRequest,
    FactRef, Retention, Visibility,
};
use garive_llm::{ModelInputContent, ModelInputItem, ModelRole};

fn candidate(position: u64, text: &str, retention: Retention) -> ContextCandidate {
    ContextCandidate {
        fact_ref: FactRef {
            session_id: "session".into(),
            position,
        },
        kind: CandidateKind::UserInput,
        retention,
        visibility: Visibility::Visible,
        items: vec![ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text(text.into())],
        }],
    }
}

fn request(max_items: usize, max_utf8_bytes: usize) -> ContextRequest {
    ContextRequest {
        session_id: "session".into(),
        turn_id: "turn".into(),
        purpose: ContextPurpose::Inference,
        after_position: None,
        through_position: 4,
        max_items,
        max_utf8_bytes,
    }
}

#[test]
fn selection_invariants_hold_across_small_budget_space() {
    let candidates = vec![
        candidate(1, "r", Retention::Required),
        candidate(2, "aa", Retention::Optional),
        candidate(3, "bbb", Retention::Optional),
        candidate(4, "cccc", Retention::Optional),
    ];
    for max_items in 1..=4 {
        for max_bytes in 1..=10 {
            let surface = derive_context(&request(max_items, max_bytes), &candidates).unwrap();
            assert_eq!(
                surface,
                derive_context(&request(max_items, max_bytes), &candidates).unwrap()
            );
            assert!(surface.item_count <= max_items);
            assert!(surface.utf8_bytes <= max_bytes);
            assert_eq!(surface.retained_refs[0].position, 1);
            assert!(surface
                .retained_refs
                .windows(2)
                .all(|pair| pair[0].position < pair[1].position));
            let retained: BTreeSet<_> = surface
                .retained_refs
                .iter()
                .map(|value| value.position)
                .collect();
            let dropped: BTreeSet<_> = surface
                .dropped_refs
                .iter()
                .map(|value| value.position)
                .collect();
            assert!(retained.is_disjoint(&dropped));
        }
    }
}

#[test]
fn text_content_chunking_preserves_admission_and_cost() {
    let mut combined = candidate(1, "蟹蟹", Retention::Required);
    let mut split = combined.clone();
    split.items = vec![ModelInputItem::Message {
        role: ModelRole::User,
        content: vec![
            ModelInputContent::Text("蟹".into()),
            ModelInputContent::Text("蟹".into()),
        ],
    }];
    let request = request(1, 6);
    let first = derive_context(&request, std::slice::from_ref(&combined)).unwrap();
    let second = derive_context(&request, std::slice::from_ref(&split)).unwrap();
    assert_eq!(first.retained_refs, second.retained_refs);
    assert_eq!(first.item_count, second.item_count);
    assert_eq!(first.utf8_bytes, second.utf8_bytes);
    assert!(matches!(first.items[0], ContextItem::Input { .. }));
    combined.items.clear();
    assert!(derive_context(&request, &[combined]).is_err());
}
