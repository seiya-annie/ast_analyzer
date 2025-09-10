ast_analyzer 设计文档
###1. 简介
##1.1. 问题背景
TiFlash 作为 TiKV 的下游组件，其 Proxy 模块的功能正确性高度依赖于上游 TiKV 的接口和关键内部实现。当 TiKV 仓库的 

master 分支发生代码变更时，手动审查每个 commit 以评估其对 TiFlash Proxy 的潜在影响，是一个耗时且容易出错的过程 。为了提高效率和准确性，我们需要一个自动化工具来监控 TiKV 关键代码路径的变更。

##1.2. 工具目标

ast_analyzer 是一个自动化代码变更分析工具的核心组件。它旨在扫描指定 Git 版本范围内的 Rust 源代码变更，通过解析代码的抽象语法树（AST），根据预设的监控规则，精确地识别出可能对下游组件（如 TiFlash）产生影响的高风险修改，并生成分析报告 。


###2. 设计目标

精确性: 采用基于 AST 的分析方法，比传统的文本搜索或正则表达式匹配更精确，能够深刻理解代码结构，减少漏报和误报 。



自动化: 脚本可以由开发者在本地运行，也具备集成到 CI/CD 流水线中的能力，实现变更的自动化监控 。



可扩展性: 监控规则与分析逻辑分离。所有监控规则都定义在一个独立的 TOML 配置文件中，使得添加、修改或删除监控点无需改动核心代码 。



模块化: 采用“Python + Rust”的混合架构，实现关注点分离。Python 负责流程控制，Rust 负责核心的代码分析，使得系统结构清晰，易于维护 。


###3. 系统架构
本工具采用 Python + Rust 的混合方案，由两个主要组件构成 ：

Python 主脚本 (scan_commits.py) - 流程编排器


角色: 负责整个扫描流程的控制和管理 。

职责:

解析用户输入的命令行参数（仓库路径、commit范围等） 。

使用 

GitPython 库与本地 Git 仓库交互，获取版本间的 commit 列表及文件变更 。


管理临时文件，用于存放新旧版本的文件内容 。


作为父进程调用 

ast_analyzer 子进程，并传递参数 。

收集并格式化 

ast_analyzer 的输出，生成对人类友好的最终报告 。



Rust 分析器 (ast_analyzer) - 核心分析引擎


角色: 负责具体的 Rust 源代码静态分析任务 。

职责:

接收 Python 脚本传递的文件路径和配置信息 。

使用 

syn 库将新旧两个版本的 Rust 代码字符串解析为 AST 。

根据 

tiflash_monitor.toml 配置文件中的规则，应用不同的策略（策略A, B, C）来比较两棵 AST 的差异 。


如果变更命中了规则，则向标准输出打印格式化的结果 。

<center>图1: 系统工作流程图</center>

###4. 组件详细设计
##4.1. 配置文件 (tiflash_monitor.toml)
该文件是监控规则的唯一来源，采用 TOML 格式，结构清晰。


[[strategy_a]]: 定义了策略A（函数/方法体变更）的监控规则 。

file: (字符串) 要监控的文件路径。

functions: (字符串数组) 在该文件中需要监控的函数或方法名列表。


[[strategy_b]]: 定义了策略B（函数调用点变更）的监控规则 。

file: (字符串) 要监控的文件路径。

functions: (字符串数组) 在该文件中需要监控的函数或方法名列表。


[[strategy_c]]: 定义了策略C（Trait 定义变更）的监控规则 。

file: (字符串) 要监控的文件路径。

traits: (字符串数组) 在该文件中需要监控的 Trait 名称列表。

##4.2. Rust 分析器 (ast_analyzer) 内部逻辑

启动: 通过 clap 库解析 --file, --old, --new, --config 四个命令行参数 。


配置加载: 使用 toml 和 serde 库反序列化 tiflash_monitor.toml 文件内容到 Rust 结构体中 。


AST 解析: 读取 --old 和 --new 文件的内容，并调用 syn::parse_file 将它们转换成两棵独立的 AST 。

策略应用:

根据 --file 参数的值，从加载的配置中筛选出适用于当前文件的所有规则。

为每个匹配的规则，调用相应的策略分析函数。

将所有策略函数返回的报告（字符串）汇总。


输出: 将汇总后的报告逐行打印到标准输出。如果没有命中任何规则，则无输出 。

###5. 核心工作流程
一次完整的扫描任务按以下步骤执行：

用户在命令行中执行 

scan_commits.py，并提供 TiKV 仓库路径、起始和终止 commit 。


scan_commits.py 初始化，使用 GitPython 打开仓库，并找到 ast_analyzer 可执行文件 。

脚本调用 

git rev-list（通过 GitPython）获取指定范围内的所有 commit 对象列表 。


脚本开始遍历每个 commit：
a.  它获取当前 

commit 与其父 commit 之间的差异（diff） 。


b.  从差异中筛选出被修改过的、且路径在 

tiflash_monitor.toml 中被关注的 .rs 文件 。



c.  对于每个相关文件，脚本通过 

git show 命令分别提取其在新旧两个 commit 中的完整内容，并写入两个临时文件（如 old.rs 和 new.rs） 。



d.  脚本构造并执行对 

ast_analyzer 的调用命令，例如：./ast_analyzer --file <path> --old old.rs --new new.rs --config config.toml 。


e.  ast_analyzer 执行其内部逻辑（见4.2节），并将分析结果（如果存在）打印到标准输出。
f.  

scan_commits.py 捕获 ast_analyzer 的标准输出 。

所有 commit 遍历完毕后，

scan_commits.py 将收集到的所有报告进行汇总，并以清晰的格式打印最终的分析报告 。


6. 监控策略实现
策略 A (函数/方法变更):

实现: 通过实现 syn::visit::Visit trait 的 FnVisitor 访问器，遍历新旧AST，提取所有函数（ItemFn）和方法（ImplItem::Fn）。


比较: 将目标函数/方法的AST节点用 prettyplease 库格式化回字符串，直接比较两个版本的字符串是否一致，来判断是否发生修改、新增或删除 。


策略 B (函数调用点变更):


实现: 由于纯语法分析难以精确追踪调用关系，此策略采用简化实现 。


比较: 直接在文件的原始代码字符串上，统计目标函数名出现的次数，并比较新旧版本的计数值差异，以此来近似判断调用点的增减 。

策略 C (Trait 变更):

实现: 通过 TraitVisitor 访问器找到目标 trait 的AST节点。


比较: 提取 trait 中所有方法（TraitItem::Fn）的名称，并存入两个 HashSet（哈希集合）。通过计算两个集合的差集，快速找出被新增或删除的方法 。


###7. 使用与部署
环境准备:

Python 3.x 及 

pip install GitPython 。

Rust 开发环境 (rustup, cargo)。

一个完整的 TiKV 本地 Git 仓库克隆。


编译: 在 ast_analyzer 项目目录中运行 cargo build --release，生成可执行文件 。


配置: 创建并维护 tiflash_monitor.toml 文件，定义所有监控规则 。


执行: 运行 scan_commits.py 脚本，并提供正确的参数 。

###8. 未来展望
提升策略B的精确度: 探索使用 rustc-hir 或其他更强大的静态分析库，以实现基于语义的、更精确的函数调用关系追踪。


CI/CD 深度集成: 将此工具作为 GitHub Action 或 Jenkins/GitLab CI 的一个步骤，实现对每次 Pull Request 或 push 的自动化扫描与报告 。




