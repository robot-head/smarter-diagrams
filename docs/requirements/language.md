Achieving Mermaid's five-second authoring speed alongside lossless, Illustrator-grade canvas control requires separating **what the user types** from **how the compiler stores and patches source text**. Rather than parsing text into a lossy Abstract Syntax Tree (AST) and pretty-printing the file on every canvas drag—which destroys comments, alignment spaces, and Git blame history—the language architecture pairs a **progressive LL(2) surface DSL** with a **`rowan`-style Red-Green lossless Concrete Syntax Tree (CST)** and a **typed structured edit algebra** inspired by `cartokit DM+NL`.

---

## 1. Progressive Disclosure: Four Tiers of the DSL

Every valid 3-line sketch is already a canonical semantic model. Authors add structural precision—ports, containers, cross-cutting Gestalt overlays, and spatial pins—strictly by accretion, without rewriting earlier lines.

### Tier 0 & Tier 1: Zero-Ceremony Topology to Explicit 5-Tier Ports

At Tier 0, plain `source -> target: label` lines infer nodes, semantic shapes, and `FREE` ports automatically. At Tier 1, dot-qualified cardinal sides (`.N`, `.E`, `.S`, `.W`), proportional ratios (`@0.25`), and record/slot anchors (`#pk`) map directly onto the solver's five-tier port constraint lattice:

```text
# Tier 0: Instant Mermaid-simple topology (ports default to FREE)
client -> gateway: HTTPS request
gateway -> auth: validate JWT

# Tier 1: Explicit ports, proportional anchors, and tabular record slots
vpc_prod: Production VPC {
  gateway.E -> broker.W@0.25: publish command [Protobuf]
  broker.E#orders -> worker.W: consume batch

  postgres: Primary DB [shape: sql_table] {
    port id: uuid [side: W, pos: 1]
    port org_id: uuid [side: W, pos: 2]
    port replica_wal: stream [side: E, ratio: 0.5]
  }

  auth.S -> postgres#id: verify user
  worker.S(order: 1) -> postgres#org_id: write ledger
  worker.S(order: 2) -> dlq.N: poison pill [style: dashed]
}

```

| Port Constraint Tier $\kappa(k)$ | Inline Endpoint Shorthand | Block Declaration Syntax | Lowered Solver Constraint |
| --- | --- | --- | --- |
| **1. `FREE**` | `auth` | `port rpc` | Solver chooses side $s_k \in \{N,E,S,W\}$ and offset $t_k$ |
| **2. `FIXED_SIDE**` | `auth.S` | `port err [side: S]` | Locks $s_k = S$; solver permutes tangential rank to minimize crossings |
| **3. `FIXED_ORDER**` | `worker.S(order: 2)` | `port retry [side: S, order: 2]` | Enforces clockwise perimeter inequality $t_{k_1} + \delta_{\text{port}} \le t_{k_2}$ |
| **4. `FIXED_RATIO**` | `broker.W@0.25` | `port wal [side: E, ratio: 0.5]` | Locks proportional offset $t_k = \rho_k L_s$ across container resizes |
| **5. `FIXED_POS**` | `postgres#org_id` | `port org_id [side: W, pos: 2]` | Locks exact row/slot offset $t_k = \bar{t}_k$ (in $g = 16\text{px}$ grid units) |

---

### Tier 2: Gestalt Cross-Cutting Zones & Relational Scenegraph

Strict tree hierarchies (`subgraph` in Mermaid or nested SVG groups) can only model **`Subset (encapsulation)`**, failing whenever security scopes, Raft consensus quorums, or distributed trace paths cut across container boundaries. Notice in the hypergraph comparison below how tree hierarchies (`Subset (encapsulation)`, center) require strict parent-child containment, whereas general system groupings form an Euler hypergraph (`Hypergraph (Euler diagram)`, left) with non-hierarchical shared membership (`Intersection (overlap)`, right).

Following Bluefish and GoFish's compound relational scenegraph, cross-cutting operators reference existing nodes via **declarative `Ref` proxies** rather than re-parenting them:

```text
# Cross-cutting Euler contour hull spanning inside and outside vpc_prod
zone pci_dss "PCI-DSS Scope" around [vpc_prod.auth, vpc_prod.postgres, kms_vault] {
  hull: rounded_contour
  padding: 1.5g
  tint: oklch(0.72 0.14 35 / 0.12)
}

# Gestalt perceptual alignment & equal spacing across container boundaries
align row [client, vpc_prod.gateway, vpc_prod.broker]
distribute col [vpc_prod.auth, vpc_prod.worker, dlq] [gap: 3g]

# Numbered semantic path overlay routed along existing topology edges
flow checkout_trace "P99 Critical Path" {
  client -> vpc_prod.gateway -> vpc_prod.auth -> vpc_prod.postgres#id
}

```

* **`zone` (Shared Background Hull):** Compiles into a hyperedge over `Ref` targets. Instead of adding conflicting rectangular VPSC containment walls that fight `vpc_prod`, the solver adds soft Delaunay proximity springs ($\mu_{\text{zone}}$) pulling members into a contiguous neighborhood, then renders a smooth rounded contour or padded convex hull behind the nodes.
* **`align` & `distribute` (Gestalt Operators):** Inject exact zero-slack VPSC equality constraints ($y_u = y_v$ for `align row`; $y_w - y_v = y_v - y_u$ for `distribute col`) without altering ownership in the semantic container tree.
* **`flow` (Overlay Sequence):** Projects numbered badge markers and elevated stroke contrast onto matched edges for step-through trace playback.

---

### Tier 3: Bidirectional `@pin` Annotations & `@layout` Separation

Canvas interactions serialize back into concise `@` directives quantized to **integer grid units ($1g = 16\text{px}$)** or **relational anchors**, never raw floating-point screen pixels:

```text
# Option A: Inline co-located pin (preserves trailing comment)
kms_vault: Cloud KMS @pin(x: vpc_prod.E + 4g, y: vpc_prod.auth.y) # Hardware HSM

# Option B: Clean separated @layout block at EOF for zero noise in domain diffs
@layout {
  vpc_prod.gateway @pin(grid: 12, 6)
  vpc_prod.broker  @pin(grid: 22, 6, lock: [w: 10g, h: 6g])
  (vpc_prod.gateway -> vpc_prod.broker) @route(channel_x: 17g) @label(slot: above, slide: 0.42)
}

```

---

## 2. Formal LL(2) Surface Grammar (EBNF)

The grammar uses newline-delimited statements (semicolons optional), needs only two tokens of lookahead to distinguish node declarations (`id: Label`) from edge chains (`id.E -> id.W`), and treats comments and whitespace as lossless trivia tokens:

```ebnf
Diagram        ::= Statement* EOF
Statement      ::= ZoneDecl | AlignDecl | DistributeDecl | FlowDecl
                 | LayoutBlock | EdgeStmt | NodeOrContainer

NodeOrContainer::= QualifiedId (":" LabelText)? AttrList? Annotation* Block?StmtEnd
Block          ::= "{" (PortDecl | Statement)* "}"
PortDecl       ::= "port" Ident (":" Ident)? AttrList? Annotation* StmtEnd

EdgeStmt       ::= Endpoint (EdgeOp Endpoint)+ (":" LabelText)? AttrList? Annotation* StmtEnd
EdgeOp         ::= "->" | "<->" | "--" | "==>"
Endpoint       ::= QualifiedId PortAnchor?
PortAnchor     ::= ("." Side ("@" FloatLit | "(" AttrBody ")")?)
                 | ("#" Ident)

Side           ::= "N" | "E" | "S" | "W"
ZoneDecl       ::= "zone" Ident StringLit? "around" RefList BlockOrAttrs StmtEnd
AlignDecl      ::= "align" ("row" | "col") RefList AttrList? StmtEnd
DistributeDecl ::= "distribute" ("row" | "col") RefList AttrList? StmtEnd
FlowDecl       ::= "flow" Ident StringLit? "{" EdgeChain* "}" StmtEnd

LayoutBlock    ::= "@layout" ("(" Ident ")")? "{" LayoutRule* "}" StmtEnd
LayoutRule     ::= (QualifiedId | "(" Endpoint EdgeOp Endpoint ")") Annotation+ StmtEnd

Annotation     ::= "@" ("pin" | "route" | "label" | "port" | "style") "(" AttrBody ")"
RefList        ::= "[" QualifiedId ("," QualifiedId)* ","? "]"
AttrList       ::= "[" AttrBody "]"
AttrBody       ::= KeyVal ("," KeyVal)* ","?
KeyVal         ::= Ident ":" Expr
Expr           ::= RelCoord | GridLit | FloatLit | IntLit | StringLit | QualifiedId | RefList
RelCoord       ::= QualifiedId ("." ("N"|"E"|"S"|"W"|"x"|"y"))? (("+" | "-") GridLit)?
QualifiedId    ::= Ident ("." Ident)*
GridLit        ::= (IntLit | FloatLit) "g"?
StmtEnd        ::= ("\n" | ";")+ | EOF

```

---

## 3. Lossless CST & Relational IR Architecture (Rust / Wasm)

To support sub-millisecond incremental re-parsing alongside zero-formatting-drift canvas edits, the compiler maintains **three synchronized representations**:

1. **Immutable Green Tree (`rowan::GreenNode`):** Stores exact concrete tokens (`IDENT`, `ARROW`, `AT_PIN`, `WHITESPACE`, `COMMENT`) with cached UTF-8 byte widths and zero absolute offsets, enabling structural sharing across undo/redo history snapshots.
2. **Ephemeral Red Tree + Typed AST (`SyntaxNode`):** Computes parent pointers and absolute `TextRange` byte spans on the fly, exposing zero-cost typed Rust wrappers (`ast::EdgeStmt`, `ast::Annotation`).
3. **Relational Scenegraph IR (`SceneGraphIR`):** Resolves `QualifiedId` scopes, lowers `Ref` proxies into hyperedges, and retains bidirectional `AstPtr` handles back to every originating CST node.

```rust
use rowan::{GreenNode, TextRange, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens & Lossless Trivia
    IDENT, COLON, DOT, HASH, AT, ARROW, BI_ARROW, L_BRACE, R_BRACE,
    L_BRACK, R_BRACK, L_PAREN, R_PAREN, COMMA, PLUS, MINUS,
    INT_LIT, FLOAT_LIT, GRID_UNIT, STRING_LIT,
    WHITESPACE, NEWLINE, COMMENT, ERROR,
    // Composite CST Nodes
    ROOT, NODE_DECL, PORT_DECL, EDGE_STMT, ENDPOINT, PORT_ANCHOR,
    ZONE_DECL, ALIGN_DECL, DISTRIBUTE_DECL, FLOW_DECL,
    LAYOUT_BLOCK, LAYOUT_RULE, ANNOTATION, ATTR_LIST, KEY_VAL,
}

/// Stable pointer linking a lowered solver entity back to its exact CST span
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceOrigin {
    pub primary_decl: TextRange,          // Node/Edge declaration span
    pub inline_pin: Option<TextRange>,    // Existing @pin(...) on declaration line
    pub layout_rule: Option<TextRange>,   // Entry inside @layout { ... }, if any
}

#[derive(Debug, Clone)]
pub enum PortConstraintSpec {
    Free,
    FixedSide { side: CardinalSide },
    FixedOrder { side: CardinalSide, order: u32 },
    FixedRatio { side: CardinalSide, ratio_clamped: f32 },
    FixedPos { side: CardinalSide, offset_grid: f32 },
}

#[derive(Debug, Clone)]
pub enum PinExpr {
    AbsoluteGrid { x: i32, y: i32 },
    RelativeAnchor { base_node: NodeId, anchor: AnchorAxis, delta_grid: f32 },
    AxisLock { axis: Axis2D, coord_grid: i32 },
}

/// Structured Edit Algebra shared by Canvas Direct Manipulation & LLM/MCP Agents
#[derive(Debug, Clone)]
pub enum DiagramEditOp {
    PinNode { node: NodeId, pin: PinExpr, size_lock: Option<(u32, u32)> },
    SnapAlign { axis: Axis2D, members: Vec<NodeId> },
    LockPort { node: NodeId, port: PortId, spec: PortConstraintSpec },
    NudgeRouteChannel { edge: EdgeId, segment_idx: usize, lane_grid: f32 },
    SlideLabel { edge: EdgeId, slot: LabelSlot, t_arc: f32 },
    UnpinEntities { targets: Vec<NodeId> },
}

/// Atomic UTF-8 byte splice applied directly to source text & GreenTree
#[derive(Debug, Clone)]
pub struct TextPatch {
    pub delete_range: TextRange,
    pub insert_text: String,
}

```

---

## 4. Bidirectional `@pin` Synthesis & Surgical Patching Rules

Empirical findings from `cartokit DM+NL` (OOPSLA 2026) show engineers perform over 93% of spatial adjustments via direct canvas manipulation when edits map cleanly back to code. To prevent canvas drags from polluting Git history with brittle pixel coordinates, the **Inverse Edit Synthesizer** enforces four deterministic rules:

### Rule 1: Semantic Snap Promotion Over Raw Coordinates

When a user releases a dragged node $v$ at canvas position $(x_{\text{px}}, y_{\text{px}})$, the synthesizer tests candidate semantic relations in priority order before falling back to grid coordinates:

1. **Collinear Peer Snap ($\pm 6\text{px}$):** If $\vert{}y_{\text{px}} - y_u\vert{} \le 6\text{px}$ for an adjacent sibling $u$, promote the gesture to `DiagramEditOp::SnapAlign { axis: Row, members: [u, v] }` (or append $v$ to an existing `align row [...]` list).
2. **Relative Perimeter Offset:** If $v$ sits outside container $C$ within $6g$ of border $C.E$, emit `@pin(x: C.E + 3g, y: ...)` so future expansions of $C$ push $v$ outward automatically without overlapping.
3. **Quantized Grid Fallback:** Round $(\lfloor x_{\text{px}} / 16 \rceil, \lfloor y_{\text{px}} / 16 \rceil)$ to integer grid units `@pin(grid: 14, 8)`.

### Rule 2: Trivia-Preserving Surgical Token Splicing

Given a `DiagramEditOp`, the CST Patcher inspects `SourceOrigin` in the Red Tree to generate minimal `TextPatch` spans:

* **Update Existing Annotation:** If `inline_pin` or `layout_rule` is `Some(range)`, locate the specific `KEY_VAL` child token inside `@pin(...)` and splice *only* the numeric or `RelCoord` literal tokens. Surrounding spaces, line breaks, and `# comments` remain byte-for-byte untouched.
* **Insert New Annotation (Placement Policy):**
* **Single-Node Edit (`inline` mode):** Scan backward from `NEWLINE` or `COMMENT` at the end of `primary_decl` and insert ` @pin(...)` immediately before any trailing `# comment`.
* **Multi-Select Drag (`@layout` mode):** When 3 or more nodes move together, upsert entries inside the file's trailing `@layout { ... }` block (creating one separated by a single blank line at `EOF` if absent) so core architecture statements stay untouched.



### Rule 3: Automatic Constraint Conflict Relaxation

Suppose `align row [auth, worker]` is active in the DSL ($\omega_{\text{pin}} = 10^5$ equality in VPSC) and the user drags `worker` vertically by $4g$ ($\ge 24\text{px}$ breakaway threshold). Rather than silently fighting the user's cursor or emitting a contradictory `@pin`, the synthesizer emits a **compound atomic transaction**:

1. Remove `worker` from `align row [auth, worker]` (deleting the `ALIGN_DECL` statement cleanly—including its leading indentation and trailing newline—if fewer than 2 members remain).
2. Insert `worker @pin(grid: 18, 12)` in the same undo frame.

### Walkthrough: Lossless Canvas Gesture → Minimal Git Diff

```diff
 # Core payment topology (hand-formatted with comments)
 vpc_prod: Production VPC {
   gateway.E -> broker.W@0.25: publish command   # hot path
-  auth.S    -> postgres#id:   verify user       # P99 < 4ms
+  auth.S(order: 1) -> postgres#id: verify user  # P99 < 4ms
 }

-align row [client, vpc_prod.gateway, kms_vault]
+align row [client, vpc_prod.gateway]
+
+@layout {
+  kms_vault @pin(x: vpc_prod.E + 3g, y: vpc_prod.auth.y)
+}

```

Notice what happened across two canvas gestures (dragging `auth`'s port stub to lock its perimeter rank, and dragging `kms_vault` out of the top row to sit beside `vpc_prod`):

1. The column-aligned whitespace and `# P99 < 4ms` comment on `auth.S` survived intact; only `(order: 1)` was spliced onto the `PORT_ANCHOR` CST node.
2. Pulling `kms_vault` vertically past the breakaway threshold pruned `, kms_vault` from `align row [...]` and synthesized a container-relative `@pin` inside `@layout`—producing a clean, reviewable 6-line PR diff.