## 仿真框架规格说明（v4：概念 + Rust 实现映射）

### 1. 目标与设计原则

#### 1.1 目标

- **通用性**: 用 Rust 实现一个通用、高性能的仿真引擎，对任意“依赖拓扑图”描述的系统进行行为与性能仿真。
- **上层模型类型不限**: 支持 RTL、行为级 / 事务级模型、真值表、统计 / 概率模型等，上层统一以“节点 + 依赖 + 消息”的图形式描述。

#### 1.2 设计原则

- **上层无关**: 引擎只依赖抽象接口（traits），不假设上层是 RTL 还是软件模型。
- **图驱动**: 仿真对象通过有向依赖拓扑图描述，图是引擎的“真相来源”。
- **子图 + 并行**: 全局大图拆分为多个子图（Subgraph），子图可以映射到多线程 / 多进程并行执行，通过消息通道通信。
- **双时间机制**:
  - **Tick‑Driven**: 周期驱动，适合精细流水线 / cycle‑accurate 子系统。
  - **Event‑Driven**: 事件驱动，作为通用时间引擎，承载功能级 / 事务级 / 统计 / 概率 / trace‑driven 等模型。
- **统一时间轴 + 明确对齐规则**: 所有子图共享统一时间轴（`SimTime`），event→tick 使用固定对齐规则（默认上取整），保证因果一致且实现简单。

---

### 2. 拓扑图与节点抽象

#### 2.1 全局图模型

- **全局图** `G = (V, E, C)`:
  - **V（节点集合）**: 每个节点表示一个逻辑单元（例如 RTL 模块、pipeline stage、cache、DRAM 控制器、统计模型等）。
  - **E（依赖边集合）**: `e = (u → v)` 表示节点 `v` 的某个输入端口依赖节点 `u` 的某个输出端口（组合 / 因果依赖）。
  - **C（跨子图消息通道集合）**: 表示不同子图之间的消息 / 事务 / 数据包传输，可携带容量和时延属性。

图本身只描述结构，不直接包含执行逻辑；执行逻辑由节点行为 + 时间推进机制共同决定。

#### 2.2 节点行为抽象（概念）

- 对引擎而言，每个节点是一个本地状态机，对输入变化 / 事件作出响应并产生输出 / 新事件:
  - **组合节点**（无状态）:
    - \( outputs(t) = F(inputs(t)) \)
  - **有状态节点**:
    - \( state(t^+) = G(state(t), inputs(t), event) \)
    - \( outputs(t^+) = H(state(t^+)) \)

- 节点行为的上层实现形式不限:
  - 真值表;
  - C/C++/Rust 回调函数;
  - DSL / 脚本解释执行;
  - 外部引擎（例如 Verilator 包裹的 C++ 类）。

- 时间 / 延迟语义:
  - **Tick‑Driven 节点**: 延迟通过“跨多个 tick 的状态迁移 + 本地队列 / 流水级”表达。
  - **Event‑Driven 节点**: 对于时间为 `t` 的输入事件 `e`，通过某延迟模型 `L()` 得到完成时间 `t' = t + L(...)`，在 `t'` 产生新事件。

> 统计 / 概率 / 近似模型本质上就是在 `L()` 中使用概率分布或分析公式，它们天然适合以 Event‑Driven 节点表达。

#### 2.3 节点在 Rust 中的接口映射（示意）

```rust
pub type NodeId = u64;
pub type SimTime = u64;

#[derive(Clone, Debug)]
pub enum NodeKind {
    RtlBlock,
    Behavioral,
    TruthTable,
    Probabilistic,
    Custom(String),
}

#[derive(Clone, Debug)]
pub struct NodePort {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug)]
pub struct NodeDesc {
    pub id: NodeId,
    pub kind: NodeKind,
    pub attrs: std::collections::HashMap<String, String>,
    pub inputs: Vec<NodePort>,
    pub outputs: Vec<NodePort>,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub time: SimTime,
    pub payload: EventPayload,
}

#[derive(Clone, Debug)]
pub enum EventPayload {
    Message { src: (NodeId, String), dst: (NodeId, String), data: serde_json::Value },
    Custom(String),
}

pub trait Node: Send {
    fn init(&mut self) {}

    /// Tick‑Driven 模式下，在每个 tick 边界调用
    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    /// Event‑Driven 模式下，在收到事件时调用
    fn on_event(&mut self, _event: Event) -> Vec<Event> {
        Vec::new()
    }
}
```

---

### 3. 子图划分与并行执行

#### 3.1 子图定义

- **子图** `S_k = (V_k, E_k, C_k)`:
  - `V_k ⊆ V`: 该子图内部的节点集合。
  - `E_k ⊆ E`: 子图内部的依赖边。
  - `C_k ⊆ C`: 与其他子图连接的跨子图通道（出站 / 入站）。

- 子图划分由上层工具完成（按物理布局、功能模块、负载均衡等），仿真引擎只消费已划分好的结果。

#### 3.2 时间模式: Tick vs Event

- 每个子图在配置中声明自己的时间推进模式:
  - `Tick‑Driven`: 周期驱动，配置 `tick_period = Δt_k`，适合精细流水 / 微结构精确部分。
  - `Event‑Driven`: 事件驱动，适合:
    - 内存 / 网络等事务级模型;
    - 统计 / 概率延迟模型;
    - trace‑driven 性能模型等。

#### 3.3 子图执行单元接口（Rust）

```rust
pub type SubgraphId = u32;

#[derive(Clone, Debug)]
pub enum TimeMode {
    Tick { period: SimTime },
    Event,
}

#[derive(Clone, Debug)]
pub struct SubgraphDesc {
    pub id: SubgraphId,
    pub time_mode: TimeMode,
    pub nodes: Vec<NodeId>,
}

pub trait SubgraphExecutor: Send {
    fn id(&self) -> SubgraphId;
    fn init(&mut self);

    /// 处理来自其他子图的输入事件（时间戳已按对齐规则处理）
    fn handle_incoming(&mut self, events: Vec<Event>);

    /// 将本子图本地时间推进到不小于 target，并返回向其他子图发出的事件
    fn run_until(&mut self, target: SimTime) -> Vec<Event>;

    fn export_stats(&self) -> serde_json::Value;
}
```

- **Tick 子图实现要点**:
  - 维护 `current_time: SimTime`。
  - 存储拓扑排序后的节点列表和 `Node` 实例表。
  - `run_until(target)`:
    - 循环: `while current_time + period <= target { step_tick(); current_time += period; }`
    - `step_tick()` 内按拓扑顺序调用各节点的 `on_tick()`。

- **Event 子图实现要点**:
  - 维护 `current_time: SimTime` 和本地事件优先队列（如 `BTreeMap<SimTime, Vec<Event>>`）。
  - `run_until(target)`:
    - 不断弹出 `time <= target` 的事件，调用对应节点 `on_event()`，产生的新事件插入本队列或发出。

---

### 4. 子图间消息通道与时间轴

#### 4.1 通道模型（概念）

- 子图间通道 `C_ij`: 从子图 `i` → 子图 `j`:
  - 属性:
    - `capacity`: 队列容量 / 背压（可选）。
    - `base_latency`: 基础传输延迟。
    - `timing_model`: 更复杂的网络 / 排队延迟模型（可选，通常自身也是 event‑driven 模型）。
  - 行为:
    - 子图 `i` 在时间 `t_send` 产生消息 `msg`。
    - 通道模型给出到达时间 `t_arrive = t_send + L_channel(msg)`。
    - 子图 `j` 在 `t_arrive` 之前看不到该消息，在 `t_arrive` 时将其转换为输入事件。

#### 4.2 Rust 中的通道描述（示意）

```rust
#[derive(Clone, Debug)]
pub struct ChannelDesc {
    pub src_subgraph: SubgraphId,
    pub dst_subgraph: SubgraphId,
    pub base_latency: SimTime,
    pub time_alignment: TimeAlignment,
}

#[derive(Clone, Debug)]
pub enum TimeAlignment {
    CeilToTick,         // 默认: 上取整对齐
    FloorToTick,        // 可选: 下取整对齐
    StrictTickBoundary, // 必须正好落在 tick 边界
}
```

---

### 5. 统一时间轴与 event→tick 对齐

#### 5.1 统一时间轴

- 使用统一的物理时间类型 `SimTime`（例如 ns 或“全局 cycle”）。
- 所有事件、tick 边界都用同一 `SimTime` 表示:
  - Event 子图在精细时间点产生事件（如 `t = 103ns`）。
  - Tick 子图只在离散的 tick 时间点更新（如 `t = 100ns, 110ns, 120ns, ...`）。

#### 5.2 问题: event→tick 如何对齐

- 设某 Tick 子图的周期为 `Δt_tick`。
- Event 子图在时间 `t_e` 产生消息，送往这个 Tick 子图。
- Tick 子图只能在 \( t_n = n \cdot \Delta t_\text{tick} \) 的时间点处理输入，因此需要一个映射:

\[
\text{align}(t_e) \rightarrow t_{\text{vis}}
\]

含义: 这个在 `t_e` 发生的事件，在 Tick 子图中被视为在哪个 tick 边界 `t_vis` 生效。

#### 5.3 推荐默认规则: 上取整对齐（CeilToTick）

规则:

\[
t_{\text{vis}} = \left\lceil \frac{t_e}{\Delta t_\text{tick}} \right\rceil \cdot \Delta t_\text{tick}
\]

解释:

- 事件发生在两个 tick 之间（例如 `t_e = 103ns`, `Δt_tick = 10ns`）:
  - `t_vis = ceil(103 / 10) * 10 = 110ns`。
- Tick 子图不会在 `100ns` 时看到该事件，只有在 `110ns` 时才会在该 tick 处理该输入。
- 该规则保证:
  - Tick 子图不会“提前看到未来事件”（因果正确）。
  - Event 子图内部更精细的时间行为保留为“在某个区间内发生”，只是在边界上对齐给 Tick 子图。

#### 5.4 与全局协调器的配合

- 选择全局步长 `ΔT`，通常取所有 Tick 周期的最小公倍数（简单起见可等于最小 `Δt_tick`）。
- 每一轮推进 `[T, T+ΔT]`:
  - Event 子图:
    - 在 `(T, T+ΔT]` 内自由处理事件、产生消息。
    - 所有新消息保留原始时间戳 `t_e`。
  - Tick 子图:
    - 在 `run_until(T+ΔT)` 中推进一个或多个 tick。
    - 对所有 `t_e ∈ (T, T+ΔT]` 的 event→tick 输入，根据 `CeilToTick` 对齐到 `T+ΔT` 统一处理。

实现效果:

- 在每个栅栏时间 `T+ΔT`:
  - Tick 子图一次性看到自上一栅栏以来所有 event→tick 输入。
  - 对齐规则自然落地为“上取整到下一栅栏时间”。
- 不需要全局事件优先队列或回滚机制，实现简单。

#### 5.5 其他可选对齐规则（非默认）

- **FloorToTick**:
  - 公式: \( t_{\text{vis}} = \left\lfloor \frac{t_e}{\Delta t_\text{tick}} \right\rfloor \cdot \Delta t_\text{tick} \)。
  - 适合你确信希望“event 子图内部延迟完全被视为上一拍已决定”的特殊场景。
  - 可能导致 Tick 子图在时间上比 event 子图内部逻辑“更早”看到结果，易引入因果问题，不推荐作为默认。

- **StrictTickBoundary**:
  - 要求 `t_e` 必须正好落在 tick 边界（`t_e = n * Δt_tick`）。
  - 若不满足，可选择报错或 snap 到最近 tick（这本质上退化为 Ceil/Floor）。
  - 适用于上层能够保证 event 子图只在 tick 边界对外发消息的场景，有利于调试和验证。

在实现上，对齐规则由 `ChannelDesc.time_alignment` 指定，runtime 在将跨子图事件送入 Tick 子图的 `handle_incoming` 之前完成时间戳转换。

---

### 6. 全局协调器（Simulation Engine）

#### 6.1 概念职责

- 维护全局时间 `T`。
- 管理所有子图执行单元 `SubgraphExecutor`。
- 每轮选择全局调度点 `T_next = T + ΔT`（第一版可用固定步长）。
- 在 `[T, T_next]` 内:
  - 分发上一轮产生的跨子图事件到目标子图；
  - 调用每个子图的 `run_until(T_next)`；
  - 收集新产生的跨子图事件，按通道延迟和对齐规则处理，缓存到下一轮。
- 当达到终止条件（`T ≥ max_time` 或事件耗尽等）时结束仿真。

#### 6.2 Rust 结构示意

```rust
pub struct SimulationEngine {
    pub subgraphs: Vec<Box<dyn SubgraphExecutor>>,
    pub channels: Vec<ChannelDesc>,
    pub current_time: SimTime,
    pub time_step: SimTime, // ΔT
    pending_cross_events: Vec<Event>,
}

impl SimulationEngine {
    pub fn init(&mut self) {
        for exec in &mut self.subgraphs {
            exec.init();
        }
    }

    pub fn run(&mut self, max_time: SimTime) {
        while self.current_time < max_time {
            let next_time = self.current_time + self.time_step;

            // 1) 把上一轮的跨子图事件分发给对应子图
            let arrivals = std::mem::take(&mut self.pending_cross_events);
            self.dispatch_to_subgraphs(arrivals);

            // 2) 推进所有子图到 next_time，并收集新产生的事件
            let mut outgoing = Vec::new();
            for exec in &mut self.subgraphs {
                let evts = exec.run_until(next_time);
                outgoing.extend(evts);
            }

            // 3) 按通道延迟 + 对齐规则处理，暂存为下一轮要分发的事件
            self.pending_cross_events = self.process_outgoing(outgoing);

            self.current_time = next_time;
        }
    }
}
```

> 后续可以将 `run` 内部并行化（不同子图用多线程执行），以及将通道映射和时间对齐逻辑抽象成独立模块。

---

### 7. 配置与扩展接口

#### 7.1 配置文件内容（YAML/JSON）

- **全局参数**:
  - `max_time`: 最大仿真时间。
  - `time_step`: 全局步长 `ΔT`。
  - `logging`: 日志等级、trace 选项。
  - `stats`: 统计项开关。

- **子图列表**:
  - 每个子图:
    - `id`。
    - `mode`: `"tick"` 或 `"event"`。
    - 若 `"tick"`: `tick_period`。
    - `nodes`: 该子图包含的 `NodeId` 列表。
    - 可选: 线程 / 进程亲和性。

- **通道列表**:
  - 每个通道:
    - `src_subgraph` / `dst_subgraph`。
    - `base_latency`。
    - `time_alignment`: `"CeilToTick"`（默认）、`"FloorToTick"`、`"StrictTickBoundary"`。

- **节点实现绑定**:
  - 在配置中将 `NodeId` 或 `node_type` 映射到 Rust 内具体的 `Node` 实现:
    - 如 `impl = "my_project::nodes::L1Cache"`。
  - 由工厂函数 / 注册表在 runtime 创建对应 `Box<dyn Node>`。

#### 7.2 扩展: 新增节点类型 / 模型

- 新增模型的步骤:
  - 实现 `Node` trait（根据所在子图模式实现 `on_tick` 或 `on_event`）。
  - 在节点注册表中注册其类型字符串到构造函数的映射。
  - 在配置文件中将对应节点的 `kind/impl` 指到该实现。

---

### 8. 统计与性能分析

- 每个 `SubgraphExecutor` 通过 `export_stats()` 导出统计数据（JSON 结构），例如:
  - 节点激活次数。
  - 事件处理总数。
  - 平均 / P95 / P99 延迟。
  - 队列深度分布。
  - 资源利用率等。

- `SimulationEngine` 结束时聚合所有子图统计，输出为 JSON / CSV，用于离线分析或可视化。

---

### 9. 小结

- 底层只需要两种时间机制:
  - **Tick‑Driven**: 用于需要周期精度的流水线 / 微结构。
  - **Event‑Driven**: 承载所有功能级 / 事务级 / 统计 / 概率 / trace‑driven 等模型。
- 通过统一时间轴和明确的 event→tick 对齐规则（默认上取整），可以在保证语义清晰和因果一致的前提下，简化实现。
- Rust 实现通过 `Node` trait、`SubgraphExecutor` trait 和 `SimulationEngine` 结构，将上述抽象自然落地，并支持后续并行化与扩展。 


