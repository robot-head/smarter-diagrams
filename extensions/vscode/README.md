# VS Code preview

Install the compiler with `cargo install --path . --features export`. From the repository root, start an extension development host:

```sh
code --extensionDevelopmentPath="$PWD/extensions/vscode" examples/architecture.diagram
```

Set `smarterDiagrams.binary` to the compiler executable if it is not on PATH. Run **Smarter Diagrams: Open Live Preview** from the command palette. The preview refreshes after buffer changes; compile errors appear in the Problems panel.

**Smarter Diagrams: Apply Structured Edit** accepts a JSON array of the Rust `DiagramEditOp` variants. For example:

```json
[{"PinNode":{"node":"client","pin":{"AbsoluteGrid":{"x":0,"y":0}},"size_lock":null}}]
```

The compiler preserves unrelated source bytes. The adapter applies a successful result in one undo step and rejects a result if the buffer changed during compilation. The preview disables scripts and uses a restrictive content security policy. This source adapter is not published to the VS Code Marketplace.
