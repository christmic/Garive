use garive_multiagent::{
    AssigneeSelector, DeliveryPolicy, NamedAgent, SessionRoster, MAX_NAMED_SESSION_AGENTS,
};

#[test]
fn ten_equal_named_agents_are_admitted_in_one_roster() {
    let members = (0..MAX_NAMED_SESSION_AGENTS)
        .map(|index| NamedAgent::new(format!("agent-{index}"), format!("Peer {index}")))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let roster = SessionRoster::new(members).unwrap();

    assert_eq!(roster.members().len(), 10);
    assert_eq!(
        AssigneeSelector::named("agent-7", &roster).unwrap(),
        AssigneeSelector::Named {
            agent_instance_id: "agent-7".into()
        }
    );
    assert_eq!(DeliveryPolicy::Notify, DeliveryPolicy::Notify);
}

#[test]
fn roster_and_target_boundaries_fail_closed() {
    let duplicate = NamedAgent::new("agent-1", "Peer").unwrap();
    assert!(SessionRoster::new(vec![duplicate.clone(), duplicate]).is_err());

    let eleven = (0..=MAX_NAMED_SESSION_AGENTS)
        .map(|index| NamedAgent::new(format!("agent-{index}"), format!("Peer {index}")))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(SessionRoster::new(eleven).is_err());

    let roster = SessionRoster::new(vec![NamedAgent::new("agent-1", "Peer").unwrap()]).unwrap();
    assert!(AssigneeSelector::named("agent-missing", &roster).is_err());
    assert!(AssigneeSelector::fork_self("agent-1", 0, None).is_err());
}
