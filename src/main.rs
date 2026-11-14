use std::io::{Read, Write};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "edit") {
        args.next();
        if args.next().as_deref() != Some("--operations") {
            return Err("edit requires --operations JSON".into());
        }
        let json = args.next().ok_or("missing operations JSON")?;
        if args.next().is_some() {
            return Err("unexpected edit arguments".into());
        }
        let ops: Vec<smarter_diagrams::edit::DiagramEditOp> = serde_json::from_str(&json)?;
        let mut source = String::new();
        std::io::stdin().read_to_string(&mut source)?;
        let result = smarter_diagrams::edit::apply_ops(&source, &ops, Default::default())?;
        smarter_diagrams::compile(&result.source)?;
        std::io::stdout().write_all(result.source.as_bytes())?;
        return Ok(());
    }

    if args.peek().is_some_and(|a| a == "mcp") {
        return Ok(smarter_diagrams::mcp::serve(
            std::io::stdin().lock(),
            std::io::stdout().lock(),
        )?);
    }
    let (mut input, mut output, mut view, mut theme_name, mut theme_file, mut events_arg) =
        (None, None, None, None, None, None);
    let mut format = "svg".to_string();
    let mut frames: usize = 16;
    let mut fps: u16 = 4;
    let mut options = smarter_diagrams::LayoutOptions::default();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dump-themes" => {
                let list: Vec<_> = smarter_diagrams::themes::all_builtin()
                    .into_iter()
                    .map(|t| serde_json::json!({ "id": t.id, "label": t.label, "dark": t.dark, "bg": t.bg, "css": t.css_vars() }))
                    .collect();
                println!("{}", serde_json::to_string(&list)?);
                return Ok(());
            }
            "-h" | "--help" => {
                println!(
                    "smarter-diagrams [INPUT|-] [-o OUTPUT] [--view NAME] [--theme NAME] [--theme-file PATH] [--format svg|json|check|lint|png|pdf|gif|apng] [--frames N] [--fps N] [--events E1,E2,...] [--no-explore-candidates]\nsmarter-diagrams mcp\nPNG/PDF/GIF/APNG require cargo install --features export. Omit INPUT to read stdin."
                );
                return Ok(());
            }
            "-o" | "--output" => output = Some(args.next().ok_or("missing output path")?),
            "--format" => format = args.next().ok_or("missing format")?,
            "--view" => view = Some(args.next().ok_or("missing view name")?),
            "--theme" => theme_name = Some(args.next().ok_or("missing theme name")?),
            "--theme-file" => theme_file = Some(args.next().ok_or("missing theme file path")?),
            "--frames" => {
                frames = args
                    .next()
                    .ok_or("missing frame count")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --frames integer")?
                    .clamp(1, 240);
            }
            "--fps" => {
                fps = args
                    .next()
                    .ok_or("missing fps")?
                    .parse::<u16>()
                    .map_err(|_| "invalid --fps integer")?
                    .clamp(1, 60);
            }
            "--events" => events_arg = Some(args.next().ok_or("missing --events list")?),
            "--no-explore-candidates" => options.explore_candidates = false,
            _ if a.starts_with('-') && a != "-" => return Err(format!("unknown option {a}").into()),
            _ => {
                if input.replace(a).is_some() {
                    return Err("only one input is accepted".into());
                }
            }
        }
    }
    let mut source = String::new();
    if input.as_deref().is_none_or(|p| p == "-") {
        std::io::stdin().read_to_string(&mut source)?;
    } else {
        source = std::fs::read_to_string(input.unwrap())?;
    }
    let mut d = smarter_diagrams::compile_with(&source, &options, None)?;
    if let Some(name) = view {
        d.scene = smarter_diagrams::views::project(&d.scene, &name)?;
        d.layout = smarter_diagrams::layout::layout(&d.scene, &options, None)?;
    }
    let active_theme = if let Some(path) = theme_file {
        let spec = std::fs::read_to_string(&path)?;
        Some(smarter_diagrams::themes::ThemePalette::from_base16_str(
            &spec,
        )?)
    } else if let Some(name) = theme_name {
        Some(
            smarter_diagrams::themes::by_name(&name)
                .ok_or_else(|| format!("unknown theme {name}"))?,
        )
    } else {
        None
    };
    #[cfg(not(feature = "export"))]
    let _ = (frames, fps, &events_arg);
    let render_svg = |diagram: &smarter_diagrams::Diagram| match &active_theme {
        Some(t) => diagram.to_svg_with_theme(t),
        None => diagram.to_svg(),
    };
    let result = match format.as_str() {
        "svg" => render_svg(&d).into_bytes(),
        "json" => serde_json::to_vec_pretty(&d.layout)?,
        "lint" => serde_json::to_vec_pretty(&smarter_diagrams::lint::lint(&d))?,
        "check" => format!(
            "{} nodes, {} edges; geometry valid\n",
            d.scene.nodes.len(),
            d.scene.edges.len()
        )
        .into_bytes(),
        #[cfg(feature = "export")]
        "png" => smarter_diagrams::export::png(&render_svg(&d), 1.)?,
        #[cfg(feature = "export")]
        "pdf" => smarter_diagrams::export::pdf(&render_svg(&d))?,
        #[cfg(feature = "export")]
        "gif" => {
            let svgs =
                collect_animation_frames(d, active_theme.as_ref(), frames, events_arg.as_deref())?;
            let delay_cs = (100u32 / u32::from(fps)).clamp(2, 500) as u16;
            smarter_diagrams::export::gif(&svgs, 1., delay_cs)?
        }
        #[cfg(feature = "export")]
        "apng" => {
            let svgs =
                collect_animation_frames(d, active_theme.as_ref(), frames, events_arg.as_deref())?;
            let delay_ms = (1000u32 / u32::from(fps)).clamp(20, 5000) as u16;
            smarter_diagrams::export::apng(&svgs, 1., delay_ms)?
        }
        _ => {
            return Err(format!(
                "unsupported format {format}; PNG/PDF/GIF/APNG need --features export"
            )
            .into());
        }
    };
    if let Some(path) = output {
        std::fs::write(path, result)?;
    } else {
        std::io::stdout().write_all(&result)?;
    }
    Ok(())
}

#[cfg(feature = "export")]
fn collect_animation_frames(
    d: smarter_diagrams::Diagram,
    theme: Option<&smarter_diagrams::themes::ThemePalette>,
    frames: usize,
    events_arg: Option<&str>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let has_sim = d
        .scene
        .nodes
        .iter()
        .any(|n| n.annotations.iter().any(|a| a.name == "sim"));
    let is_statechart = events_arg.is_some()
        || d.scene.nodes.iter().any(|n| {
            matches!(
                n.shape.as_str(),
                "initial" | "final" | "history" | "deep_history"
            ) || n.attrs.contains_key("parallel")
                || n.attrs.contains_key("region")
                || n.attrs.contains_key("context")
        });

    if is_statechart {
        let mut sim = has_sim
            .then(|| smarter_diagrams::simulation::Simulation::from_scene(&d.scene))
            .transpose()?;
        let mut sc = smarter_diagrams::statecharts::Statechart::from_diagram(d);
        let render_sc =
            |chart: &smarter_diagrams::statecharts::Statechart,
             sim_ref: Option<&smarter_diagrams::simulation::Simulation>| match theme {
                Some(t) => chart.to_svg_with_theme(t, sim_ref),
                None => chart.to_svg_with_simulation(sim_ref),
            };
        let mut out = vec![render_sc(&sc, sim.as_ref())];
        if let Some(ev_csv) = events_arg {
            for ev in ev_csv.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                match &mut sim {
                    Some(s) => {
                        let _ = sc.dispatch_with_simulation(ev, s)?;
                        let _ = sc.step_coupled(s, 10)?;
                    }
                    None => {
                        let _ = sc.dispatch(ev)?;
                    }
                }
                out.push(render_sc(&sc, sim.as_ref()));
            }
        } else if let Some(s) = &mut sim {
            for _ in 1..frames {
                let _ = sc.step_coupled(s, 10)?;
                out.push(render_sc(&sc, Some(s)));
            }
        } else {
            for _ in 1..frames {
                let next_ev = sc.available_events().into_iter().next();
                match next_ev {
                    Some(ev) => {
                        let _ = sc.dispatch(&ev)?;
                        out.push(render_sc(&sc, None));
                    }
                    None => break,
                }
            }
        }
        return Ok(out);
    }

    if has_sim {
        let mut sim = smarter_diagrams::simulation::Simulation::from_scene(&d.scene)?;
        let mut out = Vec::with_capacity(frames);
        out.push(smarter_diagrams::statecharts::simulation_frame_svg(
            &d, &sim,
        ));
        for _ in 1..frames {
            sim.advance(0.25)?;
            out.push(smarter_diagrams::statecharts::simulation_frame_svg(
                &d, &sim,
            ));
        }
        return Ok(out);
    }

    if let Some(flow) = d.scene.flows.first() {
        let mut out = Vec::with_capacity(flow.steps.len().max(1));
        for step in 0..flow.steps.len().max(1) {
            out.push(smarter_diagrams::render::flow_frame_svg(
                &d.scene, &d.layout, &flow.id, step,
            )?);
        }
        return Ok(out);
    }

    let single = match theme {
        Some(t) => d.to_svg_with_theme(t),
        None => d.to_svg(),
    };
    Ok(vec![single])
}
