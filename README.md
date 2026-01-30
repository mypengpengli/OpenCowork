# OpenCowork - Your AI Work Companion

OpenCowork is designed to stay simple and lightweight. It focuses on two core capabilities: screen monitoring and the Skills system. Because Skills are infinitely extensible, OpenCowork includes built-in creation, management, update, and deletion of Skills. You can ask the model to generate a Skill for any workflow (for example, a file-organization Skill), or import an existing Skill. If a Skill has flaws, you can have the model revise it or edit `SKILL.md` yourself.

[![Version](https://img.shields.io/badge/version-0.2.5-blue.svg)](https://github.com/mypengpengli/OpenCowork)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/mypengpengli/OpenCowork?style=social)](https://github.com/mypengpengli/OpenCowork)

**[English](#english) | [涓枃](#涓枃)**

---

<a name="english"></a>

> **Never miss an error message again.**

Have you ever experienced this?

- During deployment, a red error flashes by in the terminal before you can read it
- While debugging, the error message is too long and gets overwritten before you can copy it
- You want to ask AI for help but can't remember what the error was
- Constantly switching between coding and searching, breaking your flow

**OpenCowork was built to solve these pain points.**

## What Can It Do?

## Recent updates

- Rich chat rendering: Markdown (GFM tables, code blocks, lists) with safe sanitization
- Tool steps cards: backend progress steps are shown as compact cards in assistant replies
- Long output folding: lengthy messages auto-collapse with expand/collapse
- Paste screenshots directly into the chat input (Ctrl+V)
- Workspace picker for tool allowed directories (first line is default)

### 馃幆 Automatically Capture Every Error

From the moment you click "Start Monitoring", OpenCowork works like a tireless assistant, continuously watching your screen. Whether it's compilation errors, runtime exceptions, or console warnings 鈥?**every detail is recorded**.

When an error is detected, AI proactively notifies you:
- What error occurred
- Possible causes
- Suggested solutions

**No more frantically taking screenshots or copy-pasting.**

### 馃挰 Instant Recall with Natural Conversation

Forgot what that error was? Just ask:

- *"What was that error just now?"*
- *"What errors did I encounter in the last 10 minutes?"*
- *"How many compilation failures did I have this afternoon?"*

OpenCowork understands natural language and supports multi-turn conversations, like chatting with a colleague who knows your entire work history.

### ?? Intent Recognition & Proactive Assistance

OpenCowork infers your current intent and scene from on-screen context (coding, browsing, form-filling, etc.). It uses that signal to surface timely hints and suggest the most relevant Skills.


### 馃敡 Skills System 鈥?Infinitely Extensible

This is OpenCowork's most powerful feature.

**Skills are reusable AI capability modules**. You can:

- Type `/export` to export activity records as a report
- Type `/analyze` for AI to deeply analyze your work patterns
- Create custom Skills to have AI perform specific tasks

Even more powerful: **AI can automatically invoke Skills**. When you say "summarize my work today", AI automatically determines which Skill to use.

### 馃殌 2.0 New Feature: AI-Managed Skills

**This is a revolutionary update.**

In version 2.0, you no longer need to manually write SKILL.md files. Just tell AI in natural language:

- *"Create a code review skill for me"* 鈫?AI automatically generates the complete Skill
- *"Modify the export skill to support Markdown format"* 鈫?AI automatically updates the Skill
- *"Delete the test skill"* 鈫?AI automatically cleans up

**AI becomes your skill factory.** Just describe your needs, and AI will:
1. Understand your intent
2. Design the skill structure
3. Write detailed instructions
4. Automatically save to the system

This means:
- **Zero barrier to creation**: No need to understand SKILL.md format or write code
- **Rapid iteration**: Skill not working well? One sentence to improve it
- **Unlimited possibilities**: Any workflow you can imagine can become a Skill

### 馃挕 Real-World Examples

Here are some practical examples of creating Skills with just one sentence:

| Your Request | What AI Creates |
|--------------|-----------------|
| *"Create a skill to convert video formats"* | A video-convert skill that guides you through using FFmpeg to convert between MP4, AVI, MKV, etc. |
| *"I need a skill to compress images in batch"* | An image-compress skill with instructions for batch processing using ImageMagick or similar tools |
| *"Help me create a skill for generating git commit messages"* | A commit-msg skill that analyzes your changes and suggests conventional commit messages |
| *"Create a skill to explain code errors"* | An error-explain skill that provides detailed explanations and solutions for common programming errors |
| *"I want a skill to summarize meeting notes"* | A meeting-summary skill that extracts action items, decisions, and key points from your notes |

**Just say what you need, and AI handles the rest!**

**For enterprise users**: Create team-specific Skills to standardize workflows and boost collaboration.

**For individual developers**: Encapsulate common operations into Skills, completing complex tasks with a single sentence.

### 馃摑 Global Prompts 鈥?Your Personal Info, Always Ready

Tired of repeatedly telling AI your company name, department, or manager's name when filling out forms?

**Global Prompts** let you save frequently used personal information that automatically gets injected into every AI conversation:

- Save your company info: *"I work at Acme Corp, Engineering Department"*
- Save your team info: *"My manager is John Smith, my director is Jane Doe"*
- Save common formats: *"Reports should use the company template with header..."*

**How it works:**
1. Go to Settings 鈫?Global Prompts tab
2. Create prompts with names like "Personal Info" or "Company Info"
3. Toggle them on/off as needed
4. AI automatically uses this info when relevant (e.g., filling forms, writing emails)

**No more copy-pasting the same info over and over!**

### 馃 Smart Frame Skipping, Save Money

Worried about token consumption? OpenCowork uses perceptual hashing to compare frames. **When the screen hasn't changed, analysis is automatically skipped**, significantly reducing API costs while ensuring no important information is missed.

## Use Cases

| Scenario | Pain Point | OpenCowork's Solution |
|----------|------------|----------------------------|
| **Deployment** | Logs scroll too fast, errors flash by | Automatically capture and save all errors |
| **Debugging** | Error too long, can't copy in time | Complete recording, query anytime |
| **Learning** | Don't know what went wrong | AI proactively analyzes and suggests |
| **Remote Collaboration** | Hard to describe the problem | Export records to precisely reproduce issues |
| **Work Review** | Forgot what you did today | Natural language query for any time period |

## Technical Highlights

- **Tauri 2 + Rust**: Native performance, minimal resource usage
- **Vue 3 + TypeScript**: Modern frontend, smooth experience
- **Intent Recognition**: Detects user intent/scene to drive proactive tips and skill suggestions
- **Dual Model Support**: Cloud API (OpenAI/Claude) or local Ollama
- **Two-Layer Storage**: Raw records + smart aggregation, balancing detail and efficiency
- **Privacy First**: All data stored locally; screenshots are saved on disk and governed by retention settings
- **Skills Hot Reload**: Edit `SKILL.md` and the in-app list updates automatically
- **Tool Use Support**: AI can autonomously create, modify, and delete skills
- **Global Prompts**: Save personal info once, auto-inject into every conversation

## Quick Start

### Requirements

- Node.js 18+
- Rust 1.70+
- Optional: Ollama (for local models)

### Installation

```bash
git clone https://github.com/mypengpengli/OpenCowork.git
cd OpenCowork
npm install
npm run tauri dev
```

### Configure AI Model

#### Cloud API (Recommended)

1. Settings 鈫?Model Source 鈫?`API (Cloud)`
2. Select API Type: `OpenAI` / `Claude` / `Custom`
3. Enter API URL and key
4. Recommended models: `gpt-4o` or `claude-3-opus-20240229`

#### Local Ollama

```bash
ollama pull llava
```

Then select `Ollama (Local)` in settings, URL: `http://localhost:11434`

## Data Storage

```
Windows: %LOCALAPPDATA%\opencowork\data\
macOS:   ~/Library/Application Support/opencowork/data/
Linux:   ~/.local/share/opencowork/data/
```

Skills live under `<data>/skills` and edits are picked up automatically.

**Privacy Guarantee**:
- All data stored locally only
- Screenshots are stored locally for analysis and can be purged by retention settings
- Images are sent to AI providers during API calls

## Build

```bash
# Development
npm run tauri dev

# Production
npm run tauri build
```

## License

MIT

---

<a name="涓枃"></a>

# OpenCowork - 浣犵殑 AI 宸ヤ綔浼翠荆

OpenCowork 浠ョ畝鍗曡交渚夸负涓伙紝鍙彁渚涘睆骞曠洃鎺у拰 Skills 涓ゅぇ鏍稿績鑳藉姏銆傜敱浜?Skills 鍙互鏃犻檺鎵╁睍锛屽伐鍏峰唴缃簡鍒涘缓銆佺鐞嗐€佷慨鏀广€佸垹闄?Skills 鐨勫姛鑳姐€備綘鍙互璁╁ぇ妯″瀷鎸変换浣曞伐浣滈渶姹傜敓鎴?Skill锛堜緥濡傗€滄暣鐞嗘枃浠垛€濈殑 Skill锛夛紝涔熷彲浠ョ洿鎺ュ鍏ュ凡鏈?Skill锛涘鏋滃凡鏈?Skill 鏈夌憰鐤碉紝鍙互璁╁ぇ妯″瀷淇敼锛屾垨鑰呬綘鑷繁缂栬緫 `SKILL.md`銆?

> **鍐嶄篃涓嶇敤鎷呭績閿欒繃浠讳綍涓€涓姤閿欎俊鎭簡銆?*

浣犳槸鍚︽湁杩囪繖鏍风殑缁忓巻锛?

- 閮ㄧ讲椤圭洰鏃讹紝缁堢閲屼竴闂€岃繃鐨勭孩鑹叉姤閿欙紝绛変綘鍙嶅簲杩囨潵宸茬粡琚柊鏃ュ織鍒疯蛋浜?
- 璋冭瘯浠ｇ爜鏃讹紝閿欒淇℃伅澶暱锛岃繕娌℃潵寰楀強澶嶅埗灏辫瑕嗙洊浜?
- 鎯抽棶 AI 甯繖瑙ｅ喅闂锛屽嵈璁颁笉娓呭垰鎵嶇殑鎶ラ敊鍐呭鏄粈涔?
- 涓€杈瑰啓浠ｇ爜涓€杈规煡鐧惧害/Google锛屾潵鍥炲垏鎹㈢獥鍙ｆ墦鏂€濊矾

**OpenCowork 灏辨槸涓鸿В鍐宠繖浜涚棝鐐硅€岀敓鐨勩€?*

## 瀹冭兘鍋氫粈涔堬紵

## 鏈€杩戞洿鏂?

- 支持在对话框直接粘贴截图（Ctrl+V）
- 增加工作区选择按钮（允许目录第一行作为默认工作区）

- 瀵硅瘽娓叉煋澧炲己锛氭敮鎸?Markdown锛堣〃鏍?浠ｇ爜鍧?鍒楄〃锛夊苟鍋氬畨鍏ㄥ噣鍖?
- 宸ュ叿姝ラ鍗＄墖锛氭妸鍚庡彴杩囩▼姝ラ浠ュ崱鐗囧舰寮忛檮鍦ㄥ洖澶嶄笅
- 闀垮唴瀹规姌鍙狅細闀挎秷鎭嚜鍔ㄦ姌鍙狅紝鍙睍寮€/鏀惰捣

### 馃幆 鑷姩鎹曡幏姣忎竴涓敊璇?

浠庝綘鐐瑰嚮銆屽紑濮嬬洃鎺с€嶇殑閭ｄ竴鍒昏捣锛孫penCowork 灏卞儚涓€涓笉鐭ョ柌鍊︾殑鍔╂墜锛屾寔缁瀵熶綘鐨勫睆骞曘€傛棤璁烘槸缂栬瘧閿欒銆佽繍琛屾椂寮傚父銆佽繕鏄帶鍒跺彴璀﹀憡鈥斺€?*姣忎竴涓粏鑺傞兘浼氳璁板綍涓嬫潵**銆?

褰撴娴嬪埌閿欒鏃讹紝AI 浼氫富鍔ㄦ帹閫佹彁閱掞紝鍛婅瘔浣狅細
- 鍙戠敓浜嗕粈涔堥敊璇?
- 鍙兘鐨勫師鍥犳槸浠€涔?
- 寤鸿濡備綍瑙ｅ喅

**浣犲啀涔熶笉闇€瑕佹墜蹇欒剼涔卞湴鎴浘銆佸鍒剁矘璐翠簡銆?*

### 馃挰 闅忔椂鍥炴函锛岃嚜鐒跺璇?

蹇樿鍒氭墠鐨勬姤閿欏唴瀹癸紵娌″叧绯伙紝鐩存帴闂細

- *"鍒氭墠閭ｄ釜鎶ラ敊鏄粈涔堬紵"*
- *"鏈€杩?10 鍒嗛挓鎴戦亣鍒颁簡鍝簺閿欒锛?*
- *"浠婂ぉ涓嬪崍缂栬瘧澶辫触浜嗗嚑娆★紵"*

OpenCowork 鐞嗚В鑷劧璇█锛屾敮鎸佸杞璇濓紝灏卞儚鍜屼竴涓簡瑙ｄ綘鎵€鏈夋搷浣滃巻鍙茬殑鍚屼簨鑱婂ぉ涓€鏍枫€?

### 馃Л 鎰忓浘璇嗗埆涓庝富鍔ㄥ府鍔?

OpenCowork 浼氬熀浜庡睆骞曞唴瀹硅瘑鍒綋鍓嶆剰鍥惧拰鍦烘櫙锛堝缂栫爜銆佹祻瑙堛€佸～琛ㄧ瓑锛夛紝骞舵嵁姝や富鍔ㄦ彁绀恒€佹帹鑽愬悎閫傜殑鎶€鑳姐€?

### 馃敡 Skills 绯荤粺 鈥斺€?鏃犻檺鎵╁睍鐨勮兘鍔?

杩欐槸 OpenCowork 鏈€寮哄ぇ鐨勭壒鎬с€?

**Skills 鏄彲澶嶇敤鐨?AI 鑳藉姏妯″潡**锛屼綘鍙互锛?

- 杈撳叆 `/export` 灏嗘搷浣滆褰曞鍑轰负鎶ュ憡
- 杈撳叆 `/analyze` 璁?AI 娣卞害鍒嗘瀽浣犵殑宸ヤ綔妯″紡
- 鍒涘缓鑷畾涔?Skill锛岃 AI 鎸夌収浣犵殑闇€姹傛墽琛岀壒瀹氫换鍔?

鏇村己澶х殑鏄紝**AI 鍙互鑷姩璋冪敤 Skills**銆傚綋浣犺"甯垜鎬荤粨涓€涓嬩粖澶╃殑宸ヤ綔"锛孉I 浼氳嚜鍔ㄥ垽鏂渶瑕佽皟鐢ㄥ摢涓?Skill 鏉ュ畬鎴愪换鍔°€?

### 馃殌 2.0 鏂扮壒鎬э細AI 鑷富绠＄悊 Skills

**杩欐槸涓€涓潻鍛芥€х殑鏇存柊銆?*

鍦?2.0 鐗堟湰涓紝浣犱笉鍐嶉渶瑕佹墜鍔ㄧ紪鍐?SKILL.md 鏂囦欢銆傚彧闇€鐢ㄨ嚜鐒惰瑷€鍛婅瘔 AI锛?

- *"甯垜鍒涘缓涓€涓唬鐮佸鏌ユ妧鑳?* 鈫?AI 鑷姩鐢熸垚瀹屾暣鐨?Skill
- *"淇敼 export 鎶€鑳斤紝璁╁畠鏀寔 Markdown 鏍煎紡"* 鈫?AI 鑷姩鏇存柊 Skill 鍐呭
- *"鍒犻櫎 test 鎶€鑳?* 鈫?AI 鑷姩娓呯悊

**AI 鎴愪负浜嗕綘鐨勬妧鑳藉伐鍘傘€?* 浣犲彧闇€瑕佹弿杩伴渶姹傦紝AI 浼氾細
1. 鐞嗚В浣犵殑鎰忓浘
2. 璁捐鎶€鑳界粨鏋?
3. 缂栧啓璇︾粏鎸囦护
4. 鑷姩淇濆瓨鍒扮郴缁?

杩欐剰鍛崇潃锛?
- **闆堕棬妲涘垱寤?*锛氫笉闇€瑕佷簡瑙?SKILL.md 鏍煎紡锛屼笉闇€瑕佸啓浠ｇ爜
- **蹇€熻凯浠?*锛氬彂鐜版妧鑳戒笉濂界敤锛熶竴鍙ヨ瘽璁?AI 鏀硅繘
- **鏃犻檺鍙兘**锛氫换浣曚綘鑳芥兂鍒扮殑宸ヤ綔娴侊紝閮藉彲浠ュ彉鎴愪竴涓?Skill

### 馃挕 瀹為檯搴旂敤绀轰緥

浠ヤ笅鏄竴浜涗竴鍙ヨ瘽鍒涘缓 Skill 鐨勫疄闄呬緥瀛愶細

| 浣犵殑璇锋眰 | AI 鍒涘缓鐨勬妧鑳?|
|---------|--------------|
| *"鍒涘缓涓€涓浆鎹㈣棰戞牸寮忕殑鎶€鑳?* | 涓€涓?video-convert 鎶€鑳斤紝鎸囧浣犱娇鐢?FFmpeg 鍦?MP4銆丄VI銆丮KV 绛夋牸寮忎箣闂磋浆鎹?|
| *"鎴戦渶瑕佷竴涓壒閲忓帇缂╁浘鐗囩殑鎶€鑳?* | 涓€涓?image-compress 鎶€鑳斤紝鍖呭惈浣跨敤 ImageMagick 绛夊伐鍏锋壒閲忓鐞嗙殑鎸囦护 |
| *"甯垜鍒涘缓涓€涓敓鎴?git 鎻愪氦淇℃伅鐨勬妧鑳?* | 涓€涓?commit-msg 鎶€鑳斤紝鍒嗘瀽浣犵殑浠ｇ爜鍙樻洿骞跺缓璁鑼冪殑鎻愪氦淇℃伅 |
| *"鍒涘缓涓€涓В閲婁唬鐮侀敊璇殑鎶€鑳?* | 涓€涓?error-explain 鎶€鑳斤紝涓哄父瑙佺紪绋嬮敊璇彁渚涜缁嗚В閲婂拰瑙ｅ喅鏂规 |
| *"鎴戞兂瑕佷竴涓€荤粨浼氳璁板綍鐨勬妧鑳?* | 涓€涓?meeting-summary 鎶€鑳斤紝浠庝綘鐨勭瑪璁颁腑鎻愬彇琛屽姩椤广€佸喅绛栧拰瑕佺偣 |

**鍙渶璇村嚭浣犵殑闇€姹傦紝AI 甯綘鎼炲畾涓€鍒囷紒**

**瀵逛簬浼佷笟鐢ㄦ埛**锛氬彲浠ヤ负鍥㈤槦瀹氬埗涓撳睘 Skills锛岀粺涓€宸ヤ綔娴佺▼锛屾彁鍗囧崗浣滄晥鐜囥€?

**瀵逛簬涓汉寮€鍙戣€?*锛氬彲浠ユ妸甯哥敤鐨勬搷浣滃皝瑁呮垚 Skill锛屼竴鍙ヨ瘽瀹屾垚澶嶆潅浠诲姟銆?

### 馃摑 鍏ㄥ眬鎻愮ず璇?鈥?涓汉淇℃伅锛岄殢鏃跺氨缁?

鏄惁鍘屽€︿簡姣忔濉〃鍗曟椂閮借鍛婅瘔 AI 浣犵殑鍏徃鍚嶇О銆侀儴闂ㄦ垨棰嗗濮撳悕锛?

**鍏ㄥ眬鎻愮ず璇?*璁╀綘淇濆瓨甯哥敤鐨勪釜浜轰俊鎭紝鑷姩娉ㄥ叆鍒版瘡娆?AI 瀵硅瘽涓細

- 淇濆瓨鍏徃淇℃伅锛?"鎴戞槸XX鍏徃鐨勫憳宸ワ紝闅跺睘浜庢妧鏈儴"*
- 淇濆瓨鍥㈤槦淇℃伅锛?"鎴戠殑閮ㄩ棬缁忕悊鏄紶涓夛紝鍒嗙棰嗗鏄潕鍥?*
- 淇濆瓨甯哥敤鏍煎紡锛?"鎶ュ憡闇€瑕佷娇鐢ㄥ叕鍙告ā鏉匡紝鍖呭惈椤电湁..."*

**浣跨敤鏂规硶锛?*
1. 杩涘叆璁剧疆 鈫?鍏ㄥ眬鎻愮ず璇嶆爣绛鹃〉
2. 鍒涘缓鎻愮ず璇嶏紝濡?涓汉淇℃伅"鎴?鍏徃淇℃伅"
3. 鏍规嵁闇€瑕佸紑鍚垨鍏抽棴
4. AI 浼氬湪鐩稿叧鍦烘櫙鑷姩浣跨敤杩欎簺淇℃伅锛堝濉啓琛ㄥ崟銆佹挵鍐欓偖浠讹級

**鍐嶄篃涓嶇敤鍙嶅澶嶅埗绮樿创鍚屾牱鐨勪俊鎭簡锛?*

### 馃 鏅鸿兘璺冲抚锛岀渷閽辩渷蹇?

鎷呭績 Token 娑堣€楀お蹇紵OpenCowork 浣跨敤鎰熺煡鍝堝笇绠楁硶瀵规瘮鐢婚潰锛?*褰撳睆骞曟病鏈夊彉鍖栨椂鑷姩璺宠繃鍒嗘瀽**锛屽湪淇濊瘉涓嶉仐婕忎换浣曢噸瑕佷俊鎭殑鍚屾椂锛屽ぇ骞呴檷浣?API 璋冪敤鎴愭湰銆?

## 閫傜敤鍦烘櫙

| 鍦烘櫙 | 鐥涚偣 | OpenCowork 鐨勮В鍐虫柟妗?|
|------|------|---------------------------|
| **椤圭洰閮ㄧ讲** | 鏃ュ織鍒峰睆锛岄敊璇竴闂€岃繃 | 鑷姩鎹曡幏骞朵繚瀛樻墍鏈夐敊璇俊鎭?|
| **浠ｇ爜璋冭瘯** | 鎶ラ敊澶暱锛屾潵涓嶅強澶嶅埗 | 瀹屾暣璁板綍锛岄殢鏃跺洖婧煡璇?|
| **瀛︿範缂栫▼** | 涓嶇煡閬撹嚜宸卞摢閲屽仛閿欎簡 | AI 涓诲姩鍒嗘瀽閿欒鍘熷洜骞剁粰鍑哄缓璁?|
| **杩滅▼鍗忎綔** | 闅句互鎻忚堪閬囧埌鐨勯棶棰?| 瀵煎嚭鎿嶄綔璁板綍锛岀簿鍑嗚繕鍘熼棶棰樼幇鍦?|
| **宸ヤ綔澶嶇洏** | 蹇樿浠婂ぉ鍋氫簡浠€涔?| 鑷劧璇█鏌ヨ浠绘剰鏃堕棿娈电殑鎿嶄綔 |

## 鎶€鏈寒鐐?

- **Tauri 2 + Rust**锛氬師鐢熸€ц兘锛屾瀬浣庤祫婧愬崰鐢?
- **Vue 3 + TypeScript**锛氱幇浠ｅ寲鍓嶇锛屾祦鐣呬綋楠?
- **鎰忓浘璇嗗埆**锛氳瘑鍒敤鎴锋剰鍥?鍦烘櫙锛岄┍鍔ㄤ富鍔ㄦ彁绀轰笌鎶€鑳芥帹鑽?
- **鍙屾ā鍨嬫敮鎸?*锛氫簯绔?API (OpenAI/Claude) 鎴栨湰鍦?Ollama锛岀伒娲婚€夋嫨
- **涓ゅ眰瀛樺偍鏋舵瀯**锛氬師濮嬭褰?+ 鏅鸿兘鑱氬悎锛屽钩琛¤缁嗗害涓庡瓨鍌ㄦ晥鐜?
- **闅愮浼樺厛**锛氭墍鏈夋暟鎹湰鍦板瓨鍌紱鎴浘浼氫繚瀛樺湪鏈湴骞跺彈淇濈暀绛栫暐鎺у埗
- **Skills 鐑噸杞?*锛氱紪杈?`SKILL.md` 鍚庡垪琛ㄨ嚜鍔ㄥ埛鏂?
- **Tool Use 鏀寔**锛欰I 鍙嚜涓昏皟鐢ㄥ伐鍏凤紝瀹炵幇鎶€鑳界殑鍒涘缓銆佷慨鏀广€佸垹闄?
- **鍏ㄥ眬鎻愮ず璇?*锛氫繚瀛樹竴娆′釜浜轰俊鎭紝鑷姩娉ㄥ叆姣忔瀵硅瘽

## 蹇€熷紑濮?

### 鐜瑕佹眰

- Node.js 18+
- Rust 1.70+
- 鍙€夛細Ollama锛堢敤浜庢湰鍦版ā鍨嬶級

### 瀹夎

```bash
git clone https://github.com/mypengpengli/OpenCowork.git
cd OpenCowork
npm install
npm run tauri dev
```

### 閰嶇疆 AI 妯″瀷

#### 浜戠 API锛堟帹鑽愶級

1. 璁剧疆 鈫?妯″瀷鏉ユ簮 鈫?`API (浜戠)`
2. 閫夋嫨 API 绫诲瀷锛歚OpenAI` / `Claude` / `鑷畾涔塦
3. 濉啓 API 鍦板潃鍜屽瘑閽?
4. 鎺ㄨ崘妯″瀷锛歚gpt-4o` 鎴?`claude-3-opus-20240229`

#### 鏈湴 Ollama

```bash
ollama pull llava
```

鐒跺悗鍦ㄨ缃腑閫夋嫨 `Ollama (鏈湴)`锛屽湴鍧€濉?`http://localhost:11434`銆?

## 浣跨敤鏂规硶

1. **寮€濮嬬洃鎺?*锛氱偣鍑汇€屽紑濮嬬洃鎺с€嶆寜閽?
2. **姝ｅ父宸ヤ綔**锛歄penCowork 鍦ㄥ悗鍙伴粯榛樿褰?
3. **閬囧埌闂**锛氭敹鍒?AI 涓诲姩鎺ㄩ€佺殑閿欒鎻愰啋
4. **鏌ヨ鍘嗗彶**锛氱敤鑷劧璇█璇㈤棶浠讳綍鏃堕棿娈电殑鎿嶄綔
5. **浣跨敤 Skills**锛氳緭鍏?`/skill-name` 璋冪敤鐗瑰畾鑳藉姏

### 鏀寔鐨勬椂闂磋〃杈?

- `鍒氭墠`銆乣鍒氬垰` 鈫?鏈€杩?5 鍒嗛挓
- `鏈€杩慛鍒嗛挓` 鈫?鎸囧畾鍒嗛挓鏁?
- `浠婂ぉ`銆乣涓婂崍`銆乣涓嬪崍` 鈫?褰撳ぉ
- `鏄ㄥぉ` 鈫?鏈€杩?2 澶?
- `杩欏懆`銆乣鏈懆` 鈫?鏈€杩?7 澶?

## 閰嶇疆鍙傝€?

<details>
<summary>鎴睆閰嶇疆</summary>

| 璁剧疆椤?| 璇存槑 | 榛樿鍊?|
|--------|------|--------|
| 鎴睆闂撮殧 | 姣忔鎴睆鐨勯棿闅旀椂闂?| 1000ms |
| 鍘嬬缉璐ㄩ噺 | 鎴浘鍘嬬缉璐ㄩ噺 (10-100) | 80% |
| 璺宠繃鏃犲彉鍖?| 鐢婚潰鏃犲彉鍖栨椂璺宠繃璇嗗埆 | 寮€鍚?|
| 鍙樺寲鏁忔劅搴?| 鐩镐技搴﹂槇鍊?(0.5-0.99) | 0.95 |

</details>

<details>
<summary>閿欒鎻愰啋閰嶇疆</summary>

| 璁剧疆椤?| 璇存槑 | 榛樿鍊?|
|--------|------|--------|
| 鎻愰啋缃俊搴﹂槇鍊?| 鍙湁缃俊搴﹁秴杩囨鍊肩殑閿欒鎵嶄細鎻愰啋 | 0.7 |
| 鎻愰啋鍐峰嵈鏃堕棿 | 鐩稿悓閿欒鐨勬彁閱掗棿闅旓紙绉掞級 | 120 |

</details>

<details>
<summary>瀛樺偍閰嶇疆</summary>

| 璁剧疆椤?| 璇存槑 | 榛樿鍊?|
|--------|------|--------|
| 淇濈暀澶╂暟 | 鍘嗗彶鏁版嵁淇濈暀鏃堕棿 | 7 澶?|
| 涓婁笅鏂囧ぇ灏?| 瀵硅瘽鏃跺姞杞界殑鏈€澶у瓧绗︽暟 | 10000 瀛楃 |

</details>

## 鏁版嵁瀛樺偍

```
Windows: %LOCALAPPDATA%\opencowork\data\
macOS:   ~/Library/Application Support/opencowork/data/
Linux:   ~/.local/share/opencowork/data/
```

**闅愮淇濋殰**锛?
- 鎵€鏈夋暟鎹粎瀛樺偍鍦ㄦ湰鍦?
- 鎴浘浼氫繚瀛樺湪鏈湴鐢ㄤ簬鍒嗘瀽锛屽彲鎸変繚鐣欑瓥鐣ユ竻鐞?
- API 璋冪敤鏃跺浘鐗囦細鍙戦€佸埌瀵瑰簲鐨?AI 鏈嶅姟鍟?

## 甯歌闂

<details>
<summary>Token 娑堣€楀お蹇€庝箞鍔烇紵</summary>

1. 纭繚銆岃烦杩囨棤鍙樺寲銆嶅姛鑳藉凡鍚敤锛堥粯璁ゅ紑鍚級
2. 鎻愰珮銆屽彉鍖栨晱鎰熷害銆嶆暟鍊?
3. 閫傚綋澧炲姞鎴睆闂撮殧
4. 浣跨敤鏈湴 Ollama 妯″瀷

</details>

<details>
<summary>濡備綍浣跨敤鍥藉唴 API锛?/summary>

鍦?API 鍦板潃涓～鍐欏吋瀹?OpenAI 鏍煎紡鐨勬湇鍔″晢鍦板潃锛屽锛?
- 鏅鸿氨 AI锛歚https://open.bigmodel.cn/api/paas/v4`
- 閫氫箟鍗冮棶锛氬弬鑰冮樋閲屼簯鏂囨。

</details>

## 鏋勫缓

```bash
# 寮€鍙戞ā寮?
npm run tauri dev

# 鐢熶骇鏋勫缓
npm run tauri build
```

## License

MIT

