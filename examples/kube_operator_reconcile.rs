//! Demonstrate visualizing, verifying, and simulating a Kubernetes operator reconciliation loop.
//!
//! Run with:
//!   cargo run --example kube_operator_reconcile
use smarter_diagrams::{
    LayoutOptions, compile,
    formal::{self, CheckResult, Invariant, State, Transition, TransitionSystem},
    layout,
    simulation::Simulation,
    views,
};
use std::collections::BTreeMap;

fn state(name: &str, phase: &str, finalizer: &str, volume: &str) -> State {
    State {
        name: name.into(),
        variables: BTreeMap::from([
            ("phase".into(), phase.into()),
            ("finalizer".into(), finalizer.into()),
            ("volume_state".into(), volume.into()),
        ]),
    }
}

fn build_reconcile_model() -> TransitionSystem {
    let states = vec![
        state("PendingSpec", "Pending", "present", "unallocated"),
        state(
            "ProvisioningStatefulSet",
            "Provisioning",
            "present",
            "bound",
        ),
        state("ConvergedReady", "Ready", "present", "bound"),
        state("ConflictBackoff", "RequeueAfter", "present", "bound"),
        state("TerminatingDrain", "Terminating", "present", "draining"),
        state("DeletedCleanly", "Deleted", "cleared", "reclaimed"),
    ];
    let transitions = vec![
        Transition {
            from: 0,
            to: 1,
            event: "reconcile_apply_ssa".into(),
            edge: Some("e9".into()),
        },
        Transition {
            from: 1,
            to: 2,
            event: "patch_ready_status".into(),
            edge: Some("e10".into()),
        },
        Transition {
            from: 1,
            to: 3,
            event: "optimistic_lock_conflict".into(),
            edge: Some("e6".into()),
        },
        Transition {
            from: 3,
            to: 1,
            event: "workqueue_pop_retry".into(),
            edge: Some("e3".into()),
        },
        Transition {
            from: 2,
            to: 4,
            event: "deletion_timestamp_set".into(),
            edge: Some("e8".into()),
        },
        Transition {
            from: 4,
            to: 5,
            event: "detach_pvc_and_clear_finalizer".into(),
            edge: Some("e10".into()),
        },
    ];
    TransitionSystem {
        states,
        initial: vec![0],
        transitions,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string("examples/kube-operator.diagram")?;
    let diagram = compile(&source)?;
    let full_svg = diagram.to_svg();

    let mut projected = diagram.clone();
    projected.scene = views::project(&diagram.scene, "control_loop")?;
    projected.layout = layout::layout(&projected.scene, &LayoutOptions::default(), None)?;
    let loop_svg = projected.to_svg();

    let model = build_reconcile_model();
    let no_orphan_pvc = Invariant::Never {
        variable: "volume_state".into(),
        value: "orphaned".into(),
    };
    let safety = formal::check(&model, &no_orphan_pvc, 12)?;
    assert!(matches!(safety, CheckResult::SafeThrough { horizon: 12 }));

    let mut sim = Simulation::from_scene(&diagram.scene)?;
    sim.set_rate("operator.informer", 220.0)?;
    for _ in 0..80 {
        sim.tick()?;
    }
    let burst_queue = sim.nodes["operator.workqueue"].queued;
    let burst_sat = sim.nodes["operator.reconciler"].saturation;

    sim.set_rate("operator.informer", 10.0)?;
    for _ in 0..120 {
        sim.tick()?;
    }
    let settled_queue = sim.nodes["operator.workqueue"].queued;

    if let Some(dir) = std::env::args().nth(1) {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(format!("{dir}/kube-operator.svg"), &full_svg)?;
        std::fs::write(format!("{dir}/kube-operator-control-loop.svg"), &loop_svg)?;
    }

    println!(
        "Kube operator loop: {} nodes, {} edges | finalizer invariant: {} | burst workqueue={burst_queue:.1} (reconciler sat {:.0}%) -> settled={settled_queue:.1}",
        diagram.scene.nodes.len(),
        diagram.scene.edges.len(),
        safety.label(),
        burst_sat * 100.0,
    );
    Ok(())
}
