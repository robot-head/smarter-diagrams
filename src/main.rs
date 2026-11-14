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
    let (mut input, mut output, mut view, mut theme_name, mut theme_file) =
        (None, None, None, None, None);
    let mut format = "svg".to_string();
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
                    "smarter-diagrams [INPUT|-] [-o OUTPUT] [--view NAME] [--theme NAME] [--theme-file PATH] [--format svg|json|check|lint|png|pdf] [--no-explore-candidates]\nsmarter-diagrams mcp\nPNG/PDF require cargo install --features export. Omit INPUT to read stdin."
                );
                return Ok(());
            }
            "-o" | "--output" => output = Some(args.next().ok_or("missing output path")?),
            "--format" => format = args.next().ok_or("missing format")?,
            "--view" => view = Some(args.next().ok_or("missing view name")?),
            "--theme" => theme_name = Some(args.next().ok_or("missing theme name")?),
            "--theme-file" => theme_file = Some(args.next().ok_or("missing theme file path")?),
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
    let render_svg = || match &active_theme {
        Some(t) => d.to_svg_with_theme(t),
        None => d.to_svg(),
    };
    let result = match format.as_str() {
        "svg" => render_svg().into_bytes(),
        "json" => serde_json::to_vec_pretty(&d.layout)?,
        "lint" => serde_json::to_vec_pretty(&smarter_diagrams::lint::lint(&d))?,
        "check" => format!(
            "{} nodes, {} edges; geometry valid\n",
            d.scene.nodes.len(),
            d.scene.edges.len()
        )
        .into_bytes(),
        #[cfg(feature = "export")]
        "png" => smarter_diagrams::export::png(&render_svg(), 1.)?,
        #[cfg(feature = "export")]
        "pdf" => smarter_diagrams::export::pdf(&render_svg())?,
        _ => {
            return Err(
                format!("unsupported format {format}; PNG/PDF need --features export").into(),
            );
        }
    };
    if let Some(path) = output {
        std::fs::write(path, result)?;
    } else {
        std::io::stdout().write_all(&result)?;
    }
    Ok(())
}
