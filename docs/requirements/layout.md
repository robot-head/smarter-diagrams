Solving port-aware orthogonal routing, mental-map stability, and collision-free typography simultaneously as a single monolithic Mixed-Integer Nonlinear Program (MINLP) is NP-hard and far too slow for interactive authoring. Modern layout engines achieve sub-16ms incremental responsiveness by decomposing a global multi-objective energy functional into a **three-stage bilevel solver pipeline**: discrete topological ordering and port assignment first, separable quadratic compaction second, and discrete-continuous label placement with differentiable micro-polish third.

Let a diagram state be $\mathcal{S} = (\mathbf{p}, \boldsymbol{\Pi}, \mathcal{R}, \mathbf{L})$, where $\mathbf{p} = \{(x_v, y_v)\}_{v \in V}$ are node centroids, $\boldsymbol{\Pi}$ are assigned perimeter port coordinates, $\mathcal{R} = \{\gamma_e\}_{e \in E}$ are orthogonal polyline routes, and $\mathbf{L}$ are label bounding boxes. The global aesthetic cost function minimizes:

$$\mathcal{J}_{\text{total}}(\mathcal{S}) = \mathcal{J}_{\text{ortho}}(\boldsymbol{\Pi}, \mathcal{R}) + \lambda_{\text{mm}} \, \mathcal{J}_{\text{mental}}\big(\mathbf{p}, \boldsymbol{\Pi} \mid \mathcal{S}^{(t-1)}\big) + \lambda_{\text{lbl}} \, \mathcal{J}_{\text{label}}(\mathbf{L}, \mathbf{p}, \mathcal{R}) + \lambda_{\text{comp}} \, \mathcal{J}_{\text{compact}}(\mathbf{p})$$

| Pipeline Stage | Subproblem & Decision Variables | Exact Solver Formulation | Complexity / Target Budget |
| --- | --- | --- | --- |
| **Stage 1A: Macro-Shape & Ports** | Layer/grid rank, port sides $s_k \in \{N,E,S,W\}$, port permutations, orthogonal bend shapes | SAT / MaxSMT Shape-First (`domus`) + Barycentric Port Sweep | $O(\Vert{}V\Vert{} \log \Vert{}V\Vert{} + \Vert{}E\Vert{})$ sweep; bounded SAT on cyclic cores (~3–6 ms) |
| **Stage 1B: Coordinates & Stability** | Continuous node coordinates $(x_v, y_v)$ under non-overlap, port alignment, and mental-map anchors | Separable Active-Set Quadratic Program (**VPSC**) | $O(\Vert{}V\Vert{} \log \Vert{}V\Vert{})$ constraint sweep + near-linear block merge (~1–2 ms) |
| **Stage 2: Orthogonal Routing** | Object-avoiding Manhattan polylines $\gamma_e$ and parallel shared-channel track offsets | $A^*$ on Orthogonal Visibility Graph + $L/S/R$ Channel Nudging | $O(\Vert{}V\Vert{}^2)$ visibility construction; microsecond incremental $A^*$ (~2–4 ms) |
| **Stage 3: Labels & Micro-Polish** | Candidate label slot binaries $\alpha_{i,k} \in \{0,1\}$ and continuous sliding parameters $s_i \in [0,1]$ | 0–1 ILP Lagrangian Relaxation + L-BFGS Smooth SDF Polish | Conflict-graph greedy/ILP + 20 autodiff steps (~2–4 ms) |

---

## 1. Port-Aware Orthogonal Routing ($\mathcal{J}_{\text{ortho}}$)

Treating nodes as point-masses causes connectors to bunch at box centers or clip corners. Following the KIELER/ELK port model and Wybrow–Marriott–Stuckey visibility routing, every incident edge endpoint attaches to an explicit port $k \in P_v$ on node $v$ with bounding box $[x_v - \frac{w_v}{2}, x_v + \frac{w_v}{2}] \times [y_v - \frac{h_v}{2}, y_v + \frac{h_v}{2}]$.

Look at the routing lattice below: padded exclusion zones surround each blue node, orange ports emit mandatory outward stubs, and the red dashed scanlines through box corners and port coordinates induce the exact orthogonal visibility vertices needed for minimum-bend $A^*$ search.

### 1.1 Five-Tier Port Constraint Rules

Each port $k \in P_v$ declares a constraint level $\kappa(k)$ in a strict lattice of geometric freedom:

1. **`FREE`:** Side $s_k \in \{N, E, S, W\}$ and tangential offset $t_k$ are solver variables. Assign $s_k$ toward the target centroid's primary cardinal sector to save up to 2 bends.
2. **`FIXED_SIDE`:** $s_k = \bar{s}_k$ is fixed (e.g., $E$ for outputs, $W$ for inputs, $S$ for error/dead-letter branches), while order along $\bar{s}_k$ is free.
3. **`FIXED_ORDER`:** For ports $k_1, k_2$ on side $s$, clockwise perimeter rank satisfies $\text{rank}(k_1) \lt \text{rank}(k_2) \implies t_{k_1} + \delta_{\text{port}} \le t_{k_2}$, where $\delta_{\text{port}}$ is minimum port spacing.
4. **`FIXED_RATIO`:** Tangential coordinate is locked proportionally to box length $L_s$: $t_k = \rho_k L_s$ for $\rho_k \in (0, 1)$, allowing container auto-sizing without breaking port proportions.
5. **`FIXED_POS`:** Exact offset $t_k = \bar{t}_k$ (required for SQL/ER row foreign keys and memory-layout pointer slots).

**Hard Stub & Perimeter Clearance Constraints:** Every port $k$ on side $s_k$ with outward unit normal $\mathbf{n}(s_k) \in \{(0,-1), (1,0), (0,1), (-1,0)\}$ enforces a straight orthogonal exit stub before its first bend:

$$\mathbf{q}_{e,0} = \boldsymbol{\pi}(u, k), \qquad \mathbf{q}_{e,1} = \boldsymbol{\pi}(u, k) + d_{\text{stub}} \, \mathbf{n}(s_k), \qquad d_{\text{stub}} \ge \delta_{\text{margin}}$$

$$t_{\min}(s_k) + \delta_{\text{corner}} \le t_k \le L_{s_k} - \delta_{\text{corner}}, \qquad \vert{}t_{k_a} - t_{k_b}\vert{} \ge \delta_{\text{port}} \quad \forall k_a \neq k_b \text{ on } s_k$$

Furthermore, dynamic node sizing enforces $L_s \ge (\vert{}P_{v,s}\vert{} - 1)\delta_{\text{port}} + 2\delta_{\text{corner}}$ so high-fanout services expand cleanly rather than overlapping arrowheads.

### 1.2 Visibility Graph & Multi-Objective Route Cost

Inflate every node bounding box by margin $\delta_{\text{margin}}$ to form open obstacle interiors $\mathring{B}_v$. Collect the set of interesting points $I$ comprising all padded obstacle corners and port stub tips $\mathbf{q}_{e,1}$. Sweeping horizontal and vertical scan-rays from every point in $I$ until hitting an obstacle $\mathring{B}_v$ generates horizontal segments $\mathcal{H}$ and vertical segments $\mathcal{V}$, whose intersections define the **Orthogonal Visibility Graph** $VG = (V_{VG}, E_{VG})$.

Represent each routed connector $\gamma_e = (\mathbf{q}_{e,0}, \mathbf{q}_{e,1}, \dots, \mathbf{q}_{e,m_e})$ as an alternating horizontal/vertical polyline in $VG$ satisfying $\gamma_e \cap \mathring{B}_v = \emptyset$ for all $v \in V$. The total orthogonal routing cost over all edges $E$ is:

$$\mathcal{J}_{\text{ortho}}(\boldsymbol{\Pi}, \mathcal{R}) = \sum_{e \in E} \Big( w_{\text{len}} \sum_{j=1}^{m_e} \Vert{}\mathbf{q}_{e,j} - \mathbf{q}_{e,j-1}\Vert{}_1 + w_{\text{bend}} (m_e - 1) + w_{\text{back}} \, \mathbb{I}_{\text{back}}(\gamma_e) + w_{\text{center}} \, \delta_{\text{dev}}(e) \Big) + \sum_{\{e, e'\}} \Big( w_{\text{cross}} \, \chi(\gamma_e, \gamma_{e'}) + w_{\text{ovlp}} \, \Omega(\gamma_e, \gamma_{e'}) \Big)$$

* **Manhattan Length & Bend Count:** $\Vert{}\mathbf{q}_{e,j} - \mathbf{q}_{e,j-1}\Vert{}_1$ penalizes total wire ink, while $(m_e - 1)$ penalizes 90-degree turns.
* **Admissible Direction-Aware $A^*$ Heuristic:** During $A^*$ search over directed visibility states $(v, d_{\text{in}})$ toward target stub $(t, d_{\text{target}})$, the exact lower bound is $h(v, d_{\text{in}}) = \Vert{}v - t\Vert{}_1 + w_{\text{bend}} \cdot b_{\min}(v, d_{\text{in}}, t, d_{\text{target}})$, where $b_{\min} \in \{0, 1, 2, 3, 4\}$ counts the minimum mandatory 90-degree turns required to align entry vector $d_{\text{in}}$ with target arrival vector $-d_{\text{target}}$ across quadrant displacement $(t - v)$.
* **Crossing & Shared-Overlap Penalties:** $\chi(\gamma_e, \gamma_{e'})$ counts proper orthogonal intersections (naturally locked to 90 degrees by Manhattan geometry) and equals $0$ when $e, e'$ share a common source hyperedge trunk. $\Omega(\gamma_e, \gamma_{e'})$ penalizes non-hyperedge collinear segment sharing prior to channel nudging.
* **Shared-Channel Track Nudging ($L \mathbin{+\!+} S \mathbin{+\!+} R$ Rule):** Multiple routes traversing the same visibility alley ("pipe") share collinear segments in $VG$. Partition segments entering channel $c$ along pseudo-direction $\vec{d}$ into three ordered subsequences—those entering from the left ($L$), straight ($S$), and right ($R$)—and assign parallel lane offsets $O = L \mathbin{+\!+} S \mathbin{+\!+} R$ spaced by $\delta_{\text{lane}}$ inside the channel width via 1D VPSC centering. For cyclic non-planar subgraphs where sequential $A^*$ order biases crossings, solve a `domus`-style SAT shape-first pass over cycle turn-regularity constraints before metric assignment.

---

## 2. Mental-Map Stability & VPSC Compaction ($\mathcal{J}_{\text{mental}}$)

When a developer edits text from AST $G^{(t-1)}$ to $G^{(t)}$, let $V_{\cap} = V^{(t)} \cap V^{(t-1)}$ be surviving nodes with prior coordinates $\mathbf{p}^{(t-1)} = \{(x_v^0, y_v^0)\}$. Following Misue et al.'s mental-map taxonomy (orthogonal ordering + proximity), Dwyer et al.'s VPSC formulation, and Kiel's model-order secondary notation, stability is governed by four coupled terms:

$$\mathcal{J}_{\text{mental}}\big(\mathbf{p}, \boldsymbol{\Pi} \mid \mathcal{S}^{(t-1)}\big) = \min_{\boldsymbol{\Delta} \in \mathbb{R}^2} \sum_{v \in V_{\cap}} \omega_v \left\Vert{} \mathbf{p}_v - (\mathbf{p}_v^0 + \boldsymbol{\Delta}) \right\Vert{}_2^2 + \mu_{\text{ord}} \, \mathcal{D}_{\tau}\big(\mathbf{p}, \mathbf{p}^0\big) + \mu_{\text{prox}} \sum_{(u,v) \in E_{\text{DT}}^0} \left( \frac{\Vert{}\mathbf{p}_u - \mathbf{p}_v\Vert{}_1}{\Vert{}\mathbf{p}_u^0 - \mathbf{p}_v^0\Vert{}_1} - 1 \right)^2 + \mu_{\text{dsl}} \, \mathcal{I}_{\text{model}}(\mathbf{p})$$

1. **Translation-Invariant Anchoring ($\boldsymbol{\Delta}$-Adjusted $L_2^2$):** Minimizing over global shift $\boldsymbol{\Delta} = (\Delta_x, \Delta_y)$ discounts rigid translation. User-pinned nodes (`@pin`) receive weight $\omega_v = 10^5$, unchanged hub nodes receive $\omega_v \propto \deg(v)$, and newly inserted neighbors receive $\omega_v = 0$.
2. **Orthogonal Ordering Distance ($\mathcal{D}_{\tau}$):** Measures normalized Kendall-$\tau$ pairwise relative-direction inversions in $x$ and $y$ across $V_{\cap}$, ensuring a node previously *above-left* of another stays *above-left* unless topological rewiring forces a flip:

$$\mathcal{D}_{\tau}(\mathbf{p}, \mathbf{p}^0) = \sum_{\{u,v\} \subset V_{\cap}} \Big( \mathbb{I}\big[(x_u - x_v)(x_u^0 - x_v^0) \lt -\epsilon_{\text{align}}^2\big] + \mathbb{I}\big[(y_u - y_v)(y_u^0 - y_v^0) \lt -\epsilon_{\text{align}}^2\big] \Big)$$


3. **Delaunay Proximity Preservation:** Over edges $E_{\text{DT}}^0$ of the prior layout's Delaunay triangulation, penalizes local neighborhood stretching or tearing.
4. **DSL Model-Order Tie-Breaking ($\mathcal{I}_{\text{model}}$):** On initial render ($V_{\cap} = \emptyset$) or for newly added siblings $u, v \notin V_{\cap}$ sharing identical topological rank, penalize inversions against source-file declaration index $\text{decl}(u) \lt \text{decl}(v)$. This makes layout deterministic without random seeds and gives authors intuitive control via line order in Git.

### Solver Rule: Elasticized Separable VPSC Quadratic Program

To execute in $O(\vert{}V\vert{} \log \vert{}V\vert{})$ time, decompose compaction into horizontal ($x$) and vertical ($y$) **Variable Placement with Separation Constraints (VPSC)** active-set Quadratic Programs using a red-black tree scanline sweep. Crucially, separate **hard geometric validity** from **elastic mental-map ordering** via non-negative slack variables $\xi_{uv}^x, \xi_{uv}^y \ge 0$:

$$\min_{\mathbf{x}, \boldsymbol{\xi}^x} \sum_{v \in V_{\cap}} \omega_v (x_v - \tilde{x}_v^0)^2 + \lambda_{\text{comp}} \sum_{(u,v) \in E} \Vert{}x_u - x_v\Vert{}_1 + M_{\text{flip}} \sum_{(u,v) \in \mathcal{O}_x^0} \xi_{uv}^x$$

subject to:

* **Hard Non-Overlap Separation (Scanline Sweep):** $x_u + \frac{w_u + w_v}{2} + g_{\text{pad}} \le x_v$ for adjacent open scanline intervals.
* **Recursive Compound Container Containment:** For every child $v$ inside container $C$, $x_{C,\text{left}} + p_{\text{in}} + \frac{w_v}{2} \le x_v \le x_{C,\text{right}} - p_{\text{in}} - \frac{w_v}{2}$, where container bounds $x_{C,\text{left}}, x_{C,\text{right}}$ are themselves VPSC variables pulled inward by $\lambda_{\text{comp}} (x_{C,\text{right}} - x_{C,\text{left}})$.
* **Port-to-Port Straight-Wire Alignment:** When edge $e=(u,v)$ connects single-fanout opposing ports, introduce soft zero-bend alignment variable $\zeta_e \ge 0$ with $-( \zeta_e ) \le \pi_y(u,k_1) - \pi_y(v,k_2) \le \zeta_e$, straightening 2-bend jogs into clean 0-bend horizontal arrows whenever non-overlap permits.
* **Elastic Orthogonal Ordering:** For pairs $(u,v) \in \mathcal{O}_x^0$ where $x_u^0 + \epsilon_{\text{align}} \le x_v^0$, impose $x_u \le x_v + \xi_{uv}^x$ with $\xi_{uv}^x \ge 0$. Small edits keep $\xi_{uv}^x = 0$ (zero mental-map drift), whereas major graph refactoring activates slack $\xi_{uv}^x \gt 0$ when keeping the old relative order would incur cost exceeding $M_{\text{flip}}$.

---

## 3. Collision-Free Label Placement ($\mathcal{J}_{\text{label}}$)

Mermaid centers edge labels blindly on curve midpoints, causing catastrophic text-over-line and text-over-label collisions. High-legibility labeling combines **cartographic discrete candidate selection** (Christensen–Marks–Shieber / Imhof rules) with a **differentiable Signed Distance Field (SDF) polish pass** (Penrose / *Rose*).

### 3.1 Stage 3A: Discrete Candidate 0–1 ILP (ZOLP)

For every labelable feature $i \in \mathcal{F}$ (edge protocol labels, port multiplicity badges, external small-node captions), generate $K_i$ discrete candidate rectangles $\{B_{i,1}, \dots, B_{i,K_i}\}$ plus a fallback inline pill-cutout or leader-callout state $B_{i,0}$ so the program is strictly feasible at any graph density:

* **Port Labels:** 4 external quadrant slots flanking stub $(\boldsymbol{\pi}, \mathbf{n}(s_k))$ outside the node border, ranked by Imhof preference (above-right preferred over below-left).
* **Edge Labels:** Sampled along the longest straight segment(s) of $\gamma_e$, scored intrinsically by three cartographic metrics:

$$c_{i,k}^{\text{prior}} = \beta_{\text{center}} \left\vert{} \frac{2 d_{\text{arc}}(B_{i,k})}{\text{len}(\gamma_e)} - 1 \right\vert{} + \beta_{\text{above}} \, \mathbb{I}_{\text{below}}(B_{i,k}) + \beta_{\text{dist}} \left( d_\perp(B_{i,k}, \gamma_e) - d_{\text{ideal}} \right)^2 + \beta_{\text{bend}} \, \mathbb{I}_{\text{straddle}}(B_{i,k})$$



where $d_{\text{arc}}$ is arc-length to the label midpoint (`Centeredness`), $\mathbb{I}_{\text{below}}$ penalizes placement underneath horizontal segments (`Aboveness`), $d_\perp$ regulates offset distance, and $\mathbb{I}_{\text{straddle}}$ forbids straddling an orthogonal corner bend.

Introduce binary selection variables $\alpha_{i,k} \in \{0, 1\}$ and precompute pairwise spatial intersections (`LabelOver`, `NodeOver`, `EdgeOver`, `PortOver`) via an R-tree:

$$\min_{\boldsymbol{\alpha} \in \{0,1\}} \sum_{i \in \mathcal{F}} \sum_{k=0}^{K_i} \Big( c_{i,k}^{\text{prior}} + w_{\text{edge\_occ}} \, \text{Area}(B_{i,k} \cap \mathcal{R}_{\neq i}) + w_{\text{pill}} \, \mathbb{I}[k = 0] \Big) \alpha_{i,k}$$

subject to:

$$\sum_{k=0}^{K_i} \alpha_{i,k} = 1 \quad \forall i \in \mathcal{F}, \qquad \alpha_{i,k} + \alpha_{j,r} \le 1 \quad \forall (i,k) \neq (j,r) \text{ with } B_{i,k} \cap B_{j,r} \neq \emptyset, \qquad \alpha_{i,k} = 0 \quad \text{if } B_{i,k} \cap \Big(\bigcup_{v \in V} B_v \cup \boldsymbol{\Pi}\Big) \neq \emptyset$$

Solve each connected component of the overlap conflict graph exactly (components rarely exceed 8–12 mutually contesting labels in orthogonal layouts) or via Lagrangian relaxation in sub-millisecond time. When $\alpha_{i,0} = 1$ (fallback inline pill), render a background knockout halo padding the text box by $4\text{px}$ so the underlying connector breaks cleanly rather than striking through glyphs.

### 3.2 Stage 3B: Continuous Differentiable SDF Micro-Polish

With discrete topology, port order, and candidate slots fixed, parameterize each edge label's tangential slide $s_i \in [0, 1]$ along its assigned straight segment and each floating visibility segment's normal lane coordinate $u_c \in [u_{c,\min}, u_{c,\max}]$. Run 15–25 steps of L-BFGS via forward-mode Wasm autodiff on a $C^2$-smooth energy using exact rectangle-to-rectangle and rectangle-to-segment Signed Distance Fields ($\text{SDF}$):

$$\mathcal{E}_{\text{polish}}(\mathbf{s}, \mathbf{u}) = \sum_{A \neq B} w_{\text{repel}} \, \text{softplus}_\beta^2\!\Big(\tau_{\text{clear}} - \text{SDF}(A, B)\Big) + \sum_{i \in \mathcal{F}} w_{\text{anchor}} (s_i - s_{i,\text{init}})^2 + \sum_{c \in \mathcal{C}} w_{\text{sym}} \left( u_c - \frac{u_{c,\min} + u_{c,\max}}{2} \right)^2$$

where $\text{softplus}_\beta(z) = \frac{1}{\beta} \ln(1 + e^{\beta z})$ smoothly approximates $\max(0, z)$. This final continuous pass nudges labels symmetrically away from nearby bends and centers parallel connector buses inside inter-container alleys with optical precision.

---

## 4. Calibrated Lexicographic Weight Hierarchy

To prevent aesthetic trade-off pathologies (such as trading an ugly node collision to save 20px of wire length, or refusing to untangle 5 edge crossings to preserve a trivial relative coordinate), calibrate penalties across strict orders of magnitude normalized to a base grid unit $g = 16\text{px}$:

| Priority Tier | Aesthetic / Structural Term | Normalized Weight | Concrete Trade-Off Invariant |
| --- | --- | --- | --- |
| **Tier 0 (Hard)** | Node–node, node–route, port–label overlap; container bounds | $\infty$ (Hard Constraints) | Never violated under any finite objective gain |
| **Tier 1 ($10^5$)** | Explicit user coordinate / alignment locks (`@pin`, `align`) | $\omega_{\text{pin}} = 1.0 \times 10^5$ | Behaves as an immovable anchor unless physically impossible |
| **Tier 2 ($10^3$)** | Proper orthogonal edge–edge crossing $\chi(\gamma_e, \gamma_{e'})$ | $w_{\text{cross}} = 2.0 \times 10^3$ | Eliminating 1 crossing justifies adding up to ~16 bends |
| **Tier 3 ($10^2$)** | Mental-map relative-order flip $M_{\text{flip}}$; connector bend $w_{\text{bend}}$ | $M_{\text{flip}} = 450, \; w_{\text{bend}} = 120$ | Preserves prior $x/y$ node order unless rewiring saves a crossing or 4+ bends |
| **Tier 4 ($10^1$)** | Fallback pill knockout $w_{\text{pill}}$; backward flow penalty $w_{\text{back}}$ | $w_{\text{pill}} = 60, \; w_{\text{back}} = 80$ | Strongly prefers clean external label space over line knockouts |
| **Tier 5 ($10^0$)** | Manhattan wire length (per grid unit $g$); label `Centeredness` & `Aboveness` | $w_{\text{len}} = 1.5, \; \beta_{\text{center}} = 4.0$ | Breaks ties cleanly toward compact, centered, symmetric geometry |
| **Tier 6 ($10^{-1}$)** | DSL source declaration order $\mu_{\text{dsl}}$; channel centering $w_{\text{sym}}$ | $\mu_{\text{dsl}} = 0.25, \; w_{\text{sym}} = 0.1$ | Zero-cost tie-breaker yielding deterministic, seed-free Git stability |