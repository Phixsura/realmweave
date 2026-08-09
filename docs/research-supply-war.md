# Supply War 市场与设计调研 — 2026-08-09

数据来源：Steam 官方 appreviews/appdetails API 实时抓取（评论数、好评率、
Boxleiter 销量估算 = 评论 × 30~50）；评论文本关键词挖掘。
局限：网页型资料（GDC 演讲、设计者访谈）因网络策略无法抓取，本文以
硬数据 + 评论原文为主。

---

## 1. 品类版图（实测数据）

### 供应网络 PvE（我们的直接赛道）

| 游戏 | 评论数 | 好评率 | 销量估算 | 要点 |
|---|---|---|---|---|
| Creeper World 4 (2020, $20) | 4,708 | 95% | 14~24 万 | 赛道开创者系列的巅峰 |
| Creeper World IXE (2024) | 2,306 | 95% | 7~12 万 | 系列仍有生命力 |
| Creeper World 3 | 983 | 97% | 3~5 万 | |
| Particle Fleet (同作者) | 8,163 | 93% | 24~41 万 | |
| Mindustry ($10) | 27,743 | 96% | 83~139 万 | 赛道天花板（内容量取胜）|

**结论 1**：供应网络塔防是一个**真实存在、好评率极高（93-97%）、
中等规模**的赛道。Creeper World 单作 10~20 万销量 = 一人工作室
可持续；Mindustry 证明上限可到百万级。玩家忠诚度罕见地高。

### 线网铺设（禅意侧）

| 游戏 | 评论数 | 好评率 | 销量估算 |
|---|---|---|---|
| Mini Motorways | 24,293 | 96% | 73~121 万 |
| Mini Metro | 16,692 | 96% | 50~83 万 |
| Dorfromantik | 29,511 | 96% | 89~148 万 |

**结论 2**："铺线"这个动词本身被百万级市场验证过。但注意评论关键词
（Mini Motorways 30 条评论）：**relax×9、chill×3、zen×1 vs tense×1**
——这个市场买的是**放松**，不是压迫。

### 潮水压力（威胁侧）

| 游戏 | 评论数 | 好评率 | 销量 |
|---|---|---|---|
| They Are Billions | 51,282 | 85% | 154~256 万 | 潮水塔防大爆款 |

### 节点图 RTS（结构最像我们的先例）

| 游戏 | 数据 | 教训 |
|---|---|---|
| Eufloria (2009) | 713 评论 94% | 节点图战争玩法成立但**极小众**，美术救不了拓扑抽象 |
| Tooth and Tail (RTS-lite **PvP**) | 1,378 评论 **65% Mixed** | **PvP RTS-lite 的墓碑**：精良制作也死于匹配人口不足 |

**结论 3（最重要的负面证据）**：Tooth and Tail 65% Mixed——制作精良的
轻量 PvP RTS 死于玩家池。**我们"先 PvE、PvP 后置"的决定被市场数据
强烈支持，甚至应该改成"PvP 无限期搁置"。**

## 2. 评论文本挖掘的设计信号

**Creeper World 玩家在夸什么**（19 条评论）：
- "so satisfying to **clean up**"（推回蔓延物的满足感）
- "light base building / survival ... quick fix"（随开随玩的一局）
- 3D 视角被当成"twist"提及——立体感是卖点而非负担（对我们三层是好消息）

**Mini Motorways 玩家在夸/骂什么**（30 条）：
- 夸：relax/chill/addictive in a mellow way（低压成瘾）
- 骂："once you reach 2000 trips there isn't any more replayability...
  challenges are just difficult by being **mean**"——**难度必须来自
  有趣的问题而不是刁难**；后期重玩性靠什么要提前想

## 3. 对我们设计的七条校准（已回写 design-supply-war.md 的待办）

1. **满足感主循环需要"推回"**：CW 玩家最爱的是 push back / clean up。
   我们目前的切割者"只能击退不能消灭"违背了这个体验支柱 →
   **改**：脉冲击退不变，但新增慢性胜利手段——网络覆盖会收复
   出生点（点亮出生 rim 节点即封闭该出生口），给玩家"清图"的进度感。
2. **压力-放松节拍**：CW 的波次间歇 = 建设的禅意时刻（Mini 系证明
   铺线本身让人放松）。切片的 45 秒间歇是对的，甚至可以更长；
   骂声都指向"mean difficulty"——潜行者的惩罚逻辑必须**先教后罚**
   （第 2 波才引入，且首次出现时给提示），已在波次表中，保留。
3. **一局时长**：CW "quick fix" 体验被点名喜爱。15~25 分钟/关合适，
   但要支持"随时存档退出"（Steam 玩家习惯）。切片不做，记入 v1。
4. **PvP 从"后置"改为"无限期搁置"**（Tooth and Tail 证据）。
   服务器代码封存，设计文档不再提 PvP 路线图。
5. **三层立体是卖点**：CW4 的 3D 被当成系列进化点夸。我们的
   三层世界在美术上要往"一眼看出是立体网络"打——这是相对 Mindustry
   （纯 2D）和 CW（地形但单层逻辑）的真实差异化。
6. **内容量决定上限**：Mindustry（百万级）vs CW（十万级）的差 =
   内容广度。一人流水线做不了 Mindustry，对标 **Creeper World 的
   10~20 万份**是诚实的目标（$15 定价 → 毛收入量级 ~$200 万，
   独立可持续）。
7. **长线重玩性**（Mini Motorways 的骂点）：种子随机地图 + 每日挑战
   是本品类标配且我们的确定性引擎天然支持（种子=回放=排行榜可验证）。
   记入 v1 backlog，切片不做。

## 4. 差异化定位声明（一句话对投资人/玩家）

> Creeper World 的压力 × Mini Motorways 的铺线手感 × 独有的
> **三层立体网络与门柱咽喉**——而且敌人剪的不是你的塔，是你的**线**。

竞品守塔、守基地；我们守的是**连接本身**。这个 twist 在赛道里没有
先例（CW 敌人是地形式蔓延、Mindustry 敌人打建筑），是真实的空白。

## 5. 风险清单更新

| 风险 | 调研后评估 |
|---|---|
| 赛道不存在 | **排除**——CW 系列 15 年长青，93-97% 好评 |
| 上限太低 | 可控——诚实对标 10~20 万份而非 Mindustry |
| PvP 深水区 | **已绕开**——PvP 无限期搁置（Tooth & Tail 证据）|
| "mean difficulty" 差评雷区 | 设计对策：先教后罚 + 压力-放松节拍 |
| 后期重玩性 | 种子地图+每日挑战（引擎天然支持），v1 兑现 |
| 美术不够"立体网络感" | 灰盒后最大的投入方向，也是差异化命门 |


---

# 第二轮广域调研（补充）— 社区、负评解剖、竞品穷举

方法补充：HN Algolia API（135 评论 Mindustry 主题帖全文挖掘）、Steam
storesearch 全目录关键词穷举、100 条/款的正负评论分句解剖、itch.io
实验作品扫描、Wikipedia 背景。Reddit/SteamSpy/BGG/GDC 因 403/网络策略
不可达，已用等效来源覆盖。

## 6. 直接竞品穷举结论：机制空白得到二次确认

Steam 全目录搜索 "supply line / network defense / power grid defense /
conveyor defense / logistics defense / energy network / node capture /
sabotage strategy / repair network" —— **没有任何一款已发行游戏以
"敌人切断你的网络线路"为核心机制**。最接近的都是断线=自己规划失误
（Mini Motorways）或敌人打建筑（Mindustry/TAB）。itch.io 上有零星
game jam 级 "Supply Lines" 实验（无成品化）。
→ "守连接而非守建筑" 的定位在两轮调研后仍然成立，且几乎可以确认是
**未被占据的机制原点**。风险相应明确：没有先例也意味着没有被验证，
灰盒验收就是验证。

## 7. 负评解剖学（每款 100 条负评分句提取，本轮最有价值的产出）

### Creeper World 4 的死穴：**"慢推"疲劳**
负评关键词：slow×13, mean×10, boring×9, grind×8。原话：
- "Just slowly grind forward every mission"
- "very static game ... slowly grinding back the creep"
**含义**：CW 的"推回去"满足感有个阴暗面——中后期变成无风险的缓慢
碾压。我们的收复机制（校准#1）必须避免同型坑：**封口不该是耐心活，
而是一次有风险的远征**（出生口在最远端 + 途中线路暴露在下一波路径上）。

### They Are Billions 的死穴：**长局无存档 + 惩罚性节奏**
负评：boring×17, slow×17, tedious×11。原话：
- "No mission surrender or restarting is criminal in a long slow paced RTS,
  when you get no rewards for losing a mission that takes hours"
**含义**：一局 15-25 分钟 + 随时存档（校准#3）从 nice-to-have 升级为
**硬性需求**；失败要快、要有信息量（死了立刻看到死因回放），不许
"几小时后功亏一篑"。

### Mini Motorways 的死穴：**决策空虚 + RNG 甩锅**
负评：mean×16, boring×14, **rng×14**。原话：
- "There don't seem to be many meaningful decisions: the pickup and
  dropoff points are given by the level, the number of cars is given"
**含义**：两条铁律——① 每个压力必须有玩家可选的多种应对（我们的
延伸/加固/脉冲/收复四动词天然满足，保持住）；② **随机必须前置且
可读**（波次预告已设计；资源点/出生口位置开局全公开，中途不得
随机刷新——修订设计：切片中所有随机性仅存在于地图生成种子）。

## 8. Mindustry HN 帖（600 分/135 评论）的正面信号

工程师玩家群（HN = 我们 Steam 策略受众的镜像）的原话：
- "Short 1-2 hour games with a focus on wave defense" 被点名为优点
  —— 有界局长是卖点不是缺陷；
- "In Factorio defenses never really a big threat again. Mindustry the
  opposite" —— 威胁必须持续在场，不许中期解除（我们的波次递进设计
  方向正确，但要保证第 3 波仍有真实威胁）；
- "It is addictive, but it has an ending... I am much more afraid of
  Factorio which seems endlessly addictive" —— **有终局的战役 +
  可选的无尽模式**是该受众的理想结构；
- 全帖高频词 "addictive"（作为褒义）+ 父子同乐 co-op 提及
  —— co-op（非对抗多人）是本品类玩家真正想要的多人形态，
  远期如果做多人，方向是 co-op 而不是 PvP（与 Tooth&Tail 教训互证）。

## 9. 板级设计定律（两轮调研蒸馏，作为 Supply War 的宪法）

1. **守连接，不守建筑**——独占定位，一切设计围绕它；
2. **压力节拍 = 波峰紧张 + 间歇禅意**（Mini 系的 relax 与 CW 的 tense
   各取一半，靠波次呼吸实现）；
3. **推回去必须有代价**（反 CW 慢推疲劳：收复是远征不是碾压）；
4. **随机只在开局，中途零暗骰**（反 MiniMoto RNG 差评）；
5. **失败要快、可读、可重开**（反 TAB 长局惩罚：15-25 分钟 + 秒重开 +
   死因回放——确定性引擎白送的能力）;
6. **有终局**（战役关卡制；无尽模式后置为可选）；
7. **多人 = 远期 co-op，永不 PvP**。

## 10. 调研遗留缺口（诚实声明）

- GDC 演讲/开发者访谈（Mini Metro、CW 作者）因网络策略无法获取，
  一手设计意图缺失，用负评反推代偿；
- SteamSpy/BGG 数据403，销量用 Boxleiter 估算（±40% 误差）；
- 中文社区（B站/贴吧）未覆盖——若目标含中文市场应补一轮。
