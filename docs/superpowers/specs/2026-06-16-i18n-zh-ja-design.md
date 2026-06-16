# 设计 — UI 国际化（i18n：English / 简体中文 / 繁體中文 / 日本語）

> 子项目 B（来自 2026-06-16「下一步」4 项的拆解：A 部署+CI / **B i18n** / C 头像与名字）。
> 独立 spec → plan → 实现循环。后续：C（头像与名字）、A（部署 + 完整 CI）。

## 1. 目标与完成标志

给 Shirita 前端加多语言：**English（默认/fallback）+ 简体中文（zh-Hans）+ 繁體中文（zh-Hant）+ 日本語（ja）**。仅翻译 UI chrome（标签、按钮、placeholder、title、空状态、错误文案），不碰用户内容。

**完成标志**：UI 全部 chrome 文案走 i18n；首次打开按浏览器语言（`navigator.language`）选 zh-Hans/zh-Hant/ja/en；Settings 可手动切换并持久化；四语言 catalog key 对齐；`vue-tsc` + `vitest` + `vite build` 全绿。

### 已确认决策（brainstorm 结论）

- **方案 = vue-i18n + 集中式 per-locale catalog**（`locales/{en,zh-Hans,zh-Hant,ja}.ts`，命名空间嵌套；en 为真值源 + fallback）。否决：SFC 内联 `<i18n>` 块（多语言散落难同步）、自研 dict（重造轮子）。
- **locale 集 = `en` / `zh-Hans` / `zh-Hant` / `ja`**（BCP-47 script 子标签；**本轮含繁体**）。
- **首次默认 = 检测 `navigator.language`**（含 `hant` 或地区 `tw/hk/mo` → zh-Hant；其余 `zh*` → zh-Hans；`ja*` → ja；其余 → en），回退 English。
- **偏好存储 = localStorage**（key `ui.locale`），沿用既有 `ui.theme` 模式，开机同步可用；不入后端 settings（多设备同步留给后续 A 的 web 版，非本轮）。
- **切换器位置 = SettingsView**（English / 简体中文 / 繁體中文 / 日本語）。
- **范围 = 只 UI chrome**；用户内容（definition/message/模板名）、系统标识（def_type id、owner_kind 等）不翻；译文 zh-Hans/zh-Hant/ja 由实现者撰写（繁简非纯字符转换，按词汇习惯：软件/軟體、视频/影片、屏幕/螢幕、设置/設定 等）。

### 不做（YAGNI）

- 后端/服务端 i18n、API 错误消息本地化（后端返回的是 HTTP 状态码 + 英文，前端按需映射成本地化文案）。
- 日期/数字/货币的 locale 格式化（当前 UI 仅 token 估算与少量时间戳，保持现状）。
- 语言偏好入后端 settings 多设备同步（留给 A）。
- RTL（中日英皆 LTR）。
- 运行时按需懒加载 locale 包（四语言体量小，全量打包即可）。

## 2. 现状（已核实）

- `shirita-ui`：Vue 3 `<script setup>` + Vite 6 + TS + Tailwind v4 + Pinia + vue-router 4 + Vitest。
- **无任何 i18n**（无 vue-i18n，文案全英文硬编码于模板/脚本）。
- 视图 5 个（`HomeView`/`NewChatView`/`NewChatPromptView`/`ChatView`/`SettingsView`/`BookView`）+ 组件约 16 个（`AppShell`/`Composer`/`MessageItem`/`MessageList`/`ChatCard`/`DefinitionEditor`/`PromptTree`/`NodeRow`/`NodePicker`/`TriggerEditor`/`VariablesEditor`/`VariablesPanel`/`RegexRuleEditor`/`AvatarPicker`/`AssetPicker`/`FullscreenEditor`/`SegmentedControl`/`SliderControl`/`ToggleSwitch`）。约 62 处 placeholder/title + 大量内联模板文本。
- `main.ts` 极简：`createApp(App).use(createPinia()).use(router).mount('#app')`。
- 偏好模式既有：`stores/ui.ts` 用 localStorage（`ui.messageStyle`/`ui.theme`/`ui.background`），boot 即时读取；`stores/ui.test.ts` 已测 localStorage 持久化。
- 既有组件测试断言英文文案（如 `DefinitionEditor.test.ts` 断言 type chip `['Character','World','Prompt']`、`'zion'`；`MessageItem`/`Composer` 等）。

## 3. 依赖与初始化

- 加依赖 `vue-i18n@^10`（Vue 3，Composition API）。
- 新建 `src/i18n.ts`：
  ```ts
  import { createI18n } from 'vue-i18n'
  import en from './locales/en'
  import zhHans from './locales/zh-Hans'
  import zhHant from './locales/zh-Hant'
  import ja from './locales/ja'
  import { resolveInitialLocale } from './locales/resolve'

  export const i18n = createI18n({
    legacy: false,
    locale: resolveInitialLocale(),
    fallbackLocale: 'en',
    messages: { en, 'zh-Hans': zhHans, 'zh-Hant': zhHant, ja },
  })
  ```
- `main.ts` 加 `.use(i18n)`。
- **`AppLocale` 类型定义在 `locales/resolve.ts`**（不在 `i18n.ts`），由 `i18n.ts`/store 从 resolve 引入——避免 `i18n.ts` ↔ `resolve.ts` 的类型循环。

## 4. 初始 locale 解析与切换

- `src/locales/resolve.ts`：
  ```ts
  export type AppLocale = 'en' | 'zh-Hans' | 'zh-Hant' | 'ja'
  export const SUPPORTED: AppLocale[] = ['en', 'zh-Hans', 'zh-Hant', 'ja']
  /** 把任意 BCP-47 串映射到受支持 locale；不匹配→null。 */
  export function normalizeLocale(tag: string | null | undefined): AppLocale | null {
    if (!tag) return null
    const t = tag.toLowerCase()
    if (t.startsWith('zh')) {
      // 繁体：显式 hant，或台/港/澳地区
      if (t.includes('hant') || /\b(tw|hk|mo)\b/.test(t) || t.endsWith('-tw') || t.endsWith('-hk') || t.endsWith('-mo')) {
        return 'zh-Hant'
      }
      return 'zh-Hans'
    }
    if (t.startsWith('ja')) return 'ja'
    if (t.startsWith('en')) return 'en'
    return null
  }
  /** 启动初值：localStorage 优先，其次浏览器语言，最后 en。 */
  export function resolveInitialLocale(): AppLocale {
    const saved = normalizeLocale(localStorage.getItem('ui.locale'))
    if (saved) return saved
    const nav = typeof navigator !== 'undefined' ? navigator.language : null
    return normalizeLocale(nav) ?? 'en'
  }
  ```
- `stores/ui.ts` 加 `locale` 状态 + `setLocale(l)`（沿用 `theme` 写法）：
  ```ts
  locale: resolveInitialLocale() as AppLocale,   // 已含 localStorage→navigator→en 兜底
  // setLocale(l: AppLocale): locale.value = l; localStorage.setItem('ui.locale', l); i18n.global.locale.value = l
  ```
  （store 引入 `i18n` 实例以驱动全局 locale，`AppLocale`/`resolveInitialLocale` 从 `locales/resolve` 引入。）

## 5. Catalog 组织

- `src/locales/en.ts`（真值源）、`zh-Hans.ts`、`zh-Hant.ts`、`ja.ts`：默认导出嵌套对象，命名空间按区域：
  `common`（save/cancel/delete/duplicate/import/export/add/close…）、`shell`（导航：Chats/New/Book/Settings…）、`home`、`newChat`、`chat`、`composer`、`book`、`definition`、`prompt`、`variables`、`settings`、`import`、`errors`。
- 动态文案用插值：如 `import.summary: 'Imported: {created} created, {skipped} skipped, {overwritten} overwritten.'`（计数后接过去式动词，各计数皆合语法，无需复数）。
- **复数（spec review 追加）**：英语对可数名词敏感（`1 token` vs `2 tokens`），中/日无此变形。凡「数字 + 可数名词」串用 **vue-i18n 复数**：
  - en 用 `|` 分形：`common.tokensEstimate: '~{count} token | ~{count} tokens'`；
  - zh-Hans/zh-Hant/ja 用单形（无 `|`）：`common.tokensEstimate: '~{count} tokens'`；
  - 调用 `t('common.tokensEstimate', count)`（vue-i18n 按 count 选形；单形 locale 任意 count 都用该形）。
  - 约定：新增计数串一律走此模式，不裸拼 `{count} <noun>s`。
- **类型安全（spec review 追加，见 §5b）**：以 en 为 schema 增强 `vue-i18n` 类型，使 `t`/`$t` 的 key 有补全与拼写检查。

## 5b. Key 类型安全（spec review 追加）

裸 `$t('common.save')` 无补全/拼写检查。以 en 为 schema 增强 vue-i18n 类型：

- `src/types/vue-i18n.d.ts`：
  ```ts
  import en from '../locales/en'
  type MessageSchema = typeof en
  declare module 'vue-i18n' {
    // 让 t / $t 的 key 受 en 结构约束（补全 + 拼写检查 + 缺 key 报错）
    export interface DefineLocaleMessage extends MessageSchema {}
  }
  ```
- 脚本里用 `const { t } = useI18n()` 即获类型化 `t`（key 补全最可靠）。
- 模板 `$t` 的补全依赖 Volar 对全局 `DefineLocaleMessage` 的支持（本仓已用 vue-tsc/Volar）；即便个别模板补全不全，`vue-tsc --noEmit` 仍会对错误 key 报错（与 §8 的 parity 运行时测试互补：编译期防写错、运行时防漏译）。
- `zh-Hans.ts`/`zh-Hant.ts`/`ja.ts` 标注 `: MessageSchema` 默认导出，缺/多 key 编译期即报错。

## 6. 组件改造

- 模板内文本/属性：`{{ $t('ns.key') }}`、`:placeholder="$t('ns.key')"`、`:title="$t('ns.key')"`。
- 脚本内（动态拼装、`error.value = ...`）：`const { t } = useI18n()` 后 `t('ns.key', { count })`。
- 覆盖：导航/标题、按钮、placeholder、title（tooltip）、空状态（如「No chats yet」）、内联说明、前端产生的错误文案。
- **不改**：用户内容（definition 名/内容、message 内容、模板名、变量名）、系统标识符（def_type 的 id、owner_kind、role 字符串等逻辑值）、`data-test` 属性。
- 布局：抽取时若遇到对文本定宽（`w-[Npx]` 卡 label）的，改 `min-w`/flex，避免长译文（德/日常更长）截断（呼应既有 i18n 约定）。

## 7. 语言切换器

- `SettingsView` 增一节「Language / 语言 / 言語」：`SegmentedControl` 或原生 select，4 项 **English / 简体中文 / 繁體中文 / 日本語**（值 `en`/`zh-Hans`/`zh-Hant`/`ja`），`data-test="locale-switcher"`，绑 `ui` store `locale` + `setLocale`。Lucide `Languages` 图标。切换即时生效（`i18n.global.locale` 响应式）。
  - 注：4 项的 `SegmentedControl` 可能偏窄，必要时用 select 或允许换行（呼应「避免定宽 label」）。

## 8. 测试

- `locales/resolve.test.ts`：`normalizeLocale`（`zh-CN`→zh-Hans、`zh`→zh-Hans、`zh-Hans`→zh-Hans、`zh-TW`→zh-Hant、`zh-HK`→zh-Hant、`zh-Hant`→zh-Hant、`zh-Hant-HK`→zh-Hant、`ja-JP`→ja、`en-US`→en、`fr`→null、空→null）；`resolveInitialLocale`（localStorage `ui.locale=ja` → ja；无 localStorage + `navigator.language` stub `zh-TW` → zh-Hant；均不匹配 → en）。
- `locales/parity.test.ts`：**key 对齐** —— 递归收集 en 的全部叶子 key 路径，断言 `zh-Hans`、`zh-Hant`、`ja` 的 key 集合各自与 en **完全一致**（不多不少），防止漏译/漂移。
- `i18n.switch.test.ts`：挂载一个用 `$t` 的最小组件，`i18n.global.locale.value='zh-Hant'` 后断言渲染文案变为繁体。
- **全局测试注入（spec review 追加，不逐文件注入）**：`@vue/test-utils` 支持全局配置 `config.global.plugins`。在 `vite.config.ts` 的 `test` 加 `setupFiles: ['./src/test/setup.ts']`，setup 内：
  ```ts
  import { config } from '@vue/test-utils'
  import { i18n } from '../i18n'
  config.global.plugins = [i18n]   // 所有 mount 自动带 i18n
  ```
  这样**所有现有/未来组件测试零改动**即可用 `$t`，杜绝逐文件注入的冗余与「`$t is not a function`」漏装。默认 locale = en（`resolveInitialLocale` 在 jsdom 无 localStorage/navigator-zh 时 → en），故既有英文断言（`'Character'`/`'World'`/`'Prompt'`/`'zion'` 等）**保持不变**。
- 全绿门槛：`vue-tsc --noEmit` + `vitest run` + `vite build`。

## 9. 实现计划拆分（交由 writing-plans）

预计 3 个 plan：

1. **Plan 1 — i18n 基建 + 切换器**：装 vue-i18n、`i18n.ts`、`locales/resolve.ts`（+测）、`en.ts` 骨架（先放 `common`/`shell`/`settings` 等少量 namespace，含一个复数串示例）、`zh-Hans.ts`/`zh-Hant.ts`/`ja.ts`（标注 `: MessageSchema`）、`types/vue-i18n.d.ts`（§5b 类型增强）、`main.ts` 接入、`ui` store `locale`/`setLocale`、**`vite.config.ts` 加 `test.setupFiles` + `src/test/setup.ts` 全局注入 i18n（§8）**、SettingsView 4 项切换器、parity + switch + resolve 测试。产出：可切换语言、基建可用、测试全局带 i18n。
2. **Plan 2 — 全量串抽取（视图）**：逐视图（Home/NewChat/NewChatPrompt/Chat/Book/Settings）抽取英文 → en/zh-Hans/zh-Hant/ja，parity 测试随抽随守。
3. **Plan 3 — 全量串抽取（组件）+ 收尾**：剩余组件（Composer/MessageItem/MessageList/ChatCard/DefinitionEditor/PromptTree/…）抽取，parity 测试守全量四语言，最终 `vue-tsc`+`vitest`+`vite build` 全绿。

## 10. 风险与缓解

- **抽取遗漏**：parity 测试只保证四语言 key 对齐，不保证「无残留硬编码英文」。缓解：分 plan 逐文件过；可选加一个简单 lint/grep 检查模板内可疑英文（非阻塞）。
- **既有测试英文断言**：默认 en 下不变；仅需补 i18n 插件挂载，避免 `useI18n()` 在无 provider 时报错。
- **vue-i18n v10 与 Vite/TS**：`legacy:false` + Composition API，标准组合；如遇 `vue-i18n` 全局 `$t` 类型缺失，加 `vue-i18n` 的 volar/TS 声明或在组件用 `useI18n()`。
