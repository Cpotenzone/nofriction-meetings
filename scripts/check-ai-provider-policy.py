#!/usr/bin/env python3
"""Release source policy: noFriction has Apple on-device and one user-entered AI endpoint.

Provider presets (OpenAI, Anthropic, Meta Muse, xAI, Mistral) are UI convenience over that
endpoint: static data that pre-fills the base URL and model. This guard allows
exactly that curated table (the same ids, hosts and default models on the Mac and
iOS), and still fails on:

- a credential literal anywhere in source (key shapes, `apiKey = "..."`, Bearer literals)
- a provider marked default/active at first run, or a preset selected without a click
- a provider or AI-service host anywhere other than the two preset tables
- network calls at startup or on save (only the explicit test/refresh commands may probe)
- retired cloud transcription modules, compile-time credential injection,
  bundled environment files, network/speech APIs in the watch app, and network
  APIs in the iPhone screen-capture broadcast extension

Model-download URLs are intentionally allowed; they install local transcription
models and are not AI inference endpoints. Runtime tests remain the behavioral
verification. The signed artifact is scanned separately by
scan-release-credentials.py.
"""
import argparse, json, re, sys
from pathlib import Path

# The only provider presets allowed, and the only places their hosts may appear.
CURATED = {
    'openai':    {'base_url': 'https://api.openai.com/v1',    'key_host': 'platform.openai.com'},
    'anthropic': {'base_url': 'https://api.anthropic.com/v1', 'key_host': 'platform.claude.com'},
    'meta':      {'base_url': 'https://api.meta.ai/v1',       'key_host': 'dev.meta.ai'},
    'xai':       {'base_url': 'https://api.x.ai/v1',          'key_host': 'console.x.ai'},
    'mistral':   {'base_url': 'https://api.mistral.ai/v1',    'key_host': 'console.mistral.ai'},
}
MAC_TABLE = ('src-tauri/src/ai/providers.rs', r'pub static ENDPOINT_PRESETS: &\[EndpointPreset\] = &\[', '\n];')
IOS_TABLE = ('ios/NoFriction/AI/AIProvider.swift', r'static let all: \[AIPreset\] = \[', '\n    ]')

# Any AI-service host: retired ones (never allowed) plus the preset hosts (table only).
AI_HOSTS = re.compile(
    r'(?:api\.(?:openai\.com|anthropic\.com|deepgram\.com|gladia\.io|groq\.com|x\.ai|mistral\.ai|meta\.ai|deepseek\.com|perplexity\.ai|together\.xyz|together\.ai)'
    r'|generativelanguage\.googleapis\.com|speech\.googleapis\.com|openrouter\.ai'
    r'|platform\.openai\.com|platform\.claude\.com|console\.anthropic\.com|console\.x\.ai|console\.mistral\.ai|dev\.meta\.ai)', re.I)
RETIRED_HOSTS = re.compile(
    r'(?:api\.(?:deepgram\.com|gladia\.io|groq\.com|deepseek\.com|perplexity\.ai|together\.xyz|together\.ai)'
    r'|generativelanguage\.googleapis\.com|speech\.googleapis\.com|openrouter\.ai)', re.I)

# Credential literals. Thresholds sit above the short synthetic shapes used in
# unit tests and below every real key format.
KEY_SHAPES = re.compile(
    r'\b(?:sk-(?:proj-|svcacct-|ant-)?[A-Za-z0-9_-]{32,}|AIza[A-Za-z0-9_-]{35}\b|gsk_[A-Za-z0-9]{24,}|xai-[A-Za-z0-9_-]{24,}'
    r'|pplx-[A-Za-z0-9_-]{24,}|hf_[A-Za-z0-9]{24,}|(?:AKIA|ASIA)[A-Z0-9]{16}\b)')
NAMED_LITERAL = re.compile(r'''(?i)\b(?:api_?key|apikey|secret|access_?token|auth_?token|bearer_?token|password|credential)\w*["']?\s*[:=]\s*(?:Some\()?\s*["']([^"'\r\n]{24,})["']''')
AUTH_LITERAL = re.compile(r'''(?i)(?:Authorization|x-api-key)["']?\s*[,=:]\s*["']((?:Bearer |Token |Basic )?[A-Za-z0-9_+./=-]{24,})["']''')
PLACEHOLDER = re.compile(r'(?i)fixture|synthetic|stub|placeholder|example|test|dummy|canary|changeme|your[_-]|redacted|<|\.\.\.|\{\}')

# Network APIs the broadcast extension may never touch (URL loading, sockets,
# Network.framework, streams, web views, multipeer, CloudKit, uploads).
BROADCAST_NETWORK = re.compile(
    r'\b(?:URLSession\w*|URLRequest|NSURLConnection|NWConnection|NWListener|NWPathMonitor|NWEndpoint|NWBrowser'
    r'|CFSocket\w*|CFStreamCreate\w*|InputStream|OutputStream|Stream\.getStreamsToHost|WKWebView|MCSession|CKContainer'
    r'|sendBroadcast|uploadTask|dataTask|socket\(|connect\(|getaddrinfo|SCNetworkReachability\w*)\b'
    r'|^\s*import\s+(?:Network|WebKit|MultipeerConnectivity|CloudKit|CFNetwork)\b', re.M)

def table_block(root, spec):
    path, start, end = spec
    text = (root / path).read_text()
    m = re.search(start, text)
    if not m:
        return text, None
    stop = text.find(end, m.end())
    if stop < 0:
        return text, None
    block = text[m.end():stop]
    return text[:m.start()] + text[stop:], block

def body_after(text, signature):
    """Brace-balanced body of the first function whose signature matches."""
    m = re.search(signature, text)
    if not m:
        return None
    i = text.find('{', m.end())
    if i < 0:
        return None
    depth = 0
    for j in range(i, len(text)):
        if text[j] == '{':
            depth += 1
        elif text[j] == '}':
            depth -= 1
            if depth == 0:
                return text[i:j + 1]
    return None

def audit(root):
    errors = []
    def need(condition, message):
        if not condition:
            errors.append(message)

    # --- Mac provider table (Apple + custom only) and preset table ---------
    rust = (root / 'src-tauri/src/ai/providers.rs').read_text()
    block = rust.split('pub static PRESETS: &[Preset] = &[', 1)[1].split('\n];', 1)[0]
    ids = re.findall(r'\bid:\s*([^,]+),', block)
    need(ids == ['APPLE_PROVIDER', '"custom"'], 'Mac AI provider table must contain only Apple and custom')
    custom = block.split('id: "custom",', 1)[-1]
    need(re.search(r'base_url:\s*""\s*,', custom), 'Mac custom endpoint must start empty')
    need(not re.search(r'https?://|\bkey_prefixes:\s*&\[\s*"|\bkey_url:\s*"[^\"]', block), 'Provider table must not contain network URLs or credential routing')
    need(re.search(r'#\[derive\([^\]]*\bDefault\b[^\]]*\)\]\s*pub struct AiConfig\b', rust.replace(rust, (root / 'src-tauri/src/ai/config.rs').read_text())),
         'Mac AiConfig must derive Default (no provider active at first run)')
    config_rs = (root / 'src-tauri/src/ai/config.rs').read_text()
    need('impl Default for AiConfig' not in config_rs, 'Mac AiConfig must not hand-write a Default with a provider')

    _, mac_block = table_block(root, MAC_TABLE)
    need(mac_block is not None, 'Mac preset table missing')
    mac = {}
    if mac_block:
        for entry in re.findall(r'EndpointPreset\s*\{(.*?)\},', mac_block, re.S):
            f = dict(re.findall(r'\b(\w+):\s*"([^"]*)"', entry))
            mac[f.get('id')] = f
        need(list(mac) == list(CURATED), f'Mac preset ids must be exactly {list(CURATED)}, got {list(mac)}')
        for pid, want in CURATED.items():
            f = mac.get(pid, {})
            need(f.get('base_url') == want['base_url'], f'Mac preset {pid} base_url must be {want["base_url"]}')
            need(f.get('key_url', '').startswith('https://' + want['key_host']), f'Mac preset {pid} key_url must be on {want["key_host"]}')
            need(bool(f.get('default_model')), f'Mac preset {pid} needs a default_model')
        need(not re.search(r'\b(?:default|selected|active|preselected|is_default)\s*:', mac_block), 'Mac preset table must not mark any preset default/active/selected')
        need(not re.search(r'\b(?:key|api_key|token|secret)\s*:\s*"[^"]+"', mac_block), 'Mac preset table must not carry a key')

    # --- iOS provider table and preset table --------------------------------
    swift_full = (root / 'ios/NoFriction/AI/AIProvider.swift').read_text()
    table = re.search(r'static let all:\s*\[AIProvider\]\s*=\s*\[([^\]]+)\]', swift_full)
    need(table and set(re.findall(r'\.(\w+)', table.group(1))) == {'custom', 'apple'}, 'iOS AI provider table must contain only Apple and custom')
    need(set(re.findall(r'AIProvider\(id:\s*"([^\"]+)"', swift_full)) == {'custom', 'apple'}, 'iOS must define no named provider ids')
    _, ios_block = table_block(root, IOS_TABLE)
    need(ios_block is not None, 'iOS preset table missing')
    ios = {}
    if ios_block:
        for entry in re.findall(r'AIPreset\((.*?)\),\s*(?:\n|$)', ios_block, re.S):
            f = dict(re.findall(r'\b(\w+):\s*"([^"]*)"', entry))
            ios[f.get('id')] = f
        need(list(ios) == list(CURATED), f'iOS preset ids must be exactly {list(CURATED)}, got {list(ios)}')
        for pid, want in CURATED.items():
            f = ios.get(pid, {})
            need(f.get('baseURL') == want['base_url'], f'iOS preset {pid} baseURL must be {want["base_url"]}')
            need(f.get('keyURL', '').startswith('https://' + want['key_host']), f'iOS preset {pid} keyURL must be on {want["key_host"]}')
            need(f.get('defaultModel') and f.get('defaultModel') == mac.get(pid, {}).get('default_model'),
                 f'iOS preset {pid} default model must match the Mac table')
        need(not re.search(r'\b(?:isDefault|selected|active|preselected)\s*:', ios_block), 'iOS preset table must not mark any preset default/active/selected')
        need(not re.search(r'\b(?:key|apiKey|token|secret)\s*:\s*"[^"]+"', ios_block), 'iOS preset table must not carry a key')

    # --- Nothing selected without a click ----------------------------------
    ai_settings = (root / 'ios/NoFriction/AI/AISettings.swift').read_text()
    need(not re.search(r'activeProviderID\s*=\s*"', ai_settings), 'iOS must not assign a literal active provider')
    settings_view = (root / 'ios/NoFriction/Views/SettingsView.swift').read_text()
    need(re.search(r'var selectedCard:\s*String\?\s*\n', settings_view), 'iOS preset card selection must start nil')
    tsx = (root / 'src/features/settings/AIProviderSettings.tsx').read_text()
    need(re.search(r'useState<string \| null>\(null\)', tsx), 'Mac preset card selection must start null')
    need(not re.search(r'useState\(\s*"(?:openai|anthropic|meta|xai|mistral)"', tsx), 'Mac settings must not pre-select a preset')
    presets_ts = (root / 'src/lib/aiPresets.ts').read_text()
    need(not re.search(r'https?://', presets_ts), 'Mac preset helpers must carry no URL (the table comes from the backend)')

    # --- No network at startup or on save ----------------------------------
    init_body = body_after(config_rs, r'pub async fn init\(')
    need(init_body is not None and not re.search(r'\bclient::|reqwest|list_models|probe\(', init_body), 'Mac AI config init must not touch the network')
    lib_rs = (root / 'src-tauri/src/lib.rs').read_text()
    need(not re.search(r'ai::(?:client::(?:probe|list_models|run|complete)|commands::ai_(?:test|list_models))\s*\(', lib_rs), 'Mac startup must not call AI network functions')
    set_endpoint = body_after((root / 'src-tauri/src/ai/commands.rs').read_text(), r'pub async fn ai_set_custom_endpoint\(')
    save_key = body_after((root / 'src-tauri/src/ai/commands.rs').read_text(), r'pub async fn ai_save_key\(')
    for name, body in (('ai_set_custom_endpoint', set_endpoint), ('ai_save_key', save_key)):
        need(body is not None and not re.search(r'\bclient::|probe\(|list_models\(', body), f'Mac {name} must not probe the network on save')
    for effect in re.findall(r'useEffect\(\(\) => \{(.*?)\n    \}, \[', tsx, re.S):
        need(not re.search(r'ai\.(?:test|listModels)\(', effect), 'Mac settings must not test or list models automatically (only on click)')
    need(not re.search(r'\b(?:URLSession|AIClient)\b', ai_settings), 'iOS AISettings must not reach the network')
    need(not re.search(r'\b(?:URLSession|AIClient)\b', swift_full), 'iOS AIProvider/presets must not reach the network')
    app_swift = (root / 'ios/NoFriction/App/NoFrictionApp.swift').read_text()
    need(not re.search(r'testConnection|URLSession', app_swift), 'iOS app startup must not probe the network')
    save_body = body_after(settings_view, r'func saveEndpoint\(')
    need(save_body is not None and not re.search(r'testConnection|URLSession|AIClient', save_body), 'iOS save must not probe the network')
    for fn in ('func choose(', 'func chooseCustom(', 'func loadSaved('):
        body = body_after(settings_view, re.escape(fn))
        need(body is not None and not re.search(r'testConnection|URLSession|AIClient|saveEndpoint|settings\.save\(', body), f'iOS {fn} must only fill the form')

    # --- Transcription stays local -----------------------------------------
    trans = (root / 'src-tauri/src/transcription/mod.rs').read_text()
    enum = re.search(r'pub enum ProviderType\s*\{([^}]+)\}', trans).group(1)
    enum = re.sub(r'//[^\n]*', '', enum)
    need(re.findall(r'\b[A-Z]\w*\b', enum) == ['Local'], 'Mac transcription must expose only local execution')
    need(not re.search(r'pub mod (deepgram|gemini|gladia|google_stt)\s*;', trans), 'Retired cloud transcription modules must not compile')

    # --- Hosts only in the tables; no credential literals; no env! keys ----
    paths = []
    for folder in ('src', 'src-tauri/src', 'ios/NoFriction', 'ios/Shared', 'ios/NoFrictionWatch', 'ios/NoFrictionTests',
                   'ios/NoFrictionBroadcast', 'ios/ScreenCaptureShared'):
        paths.extend(p for p in (root / folder).rglob('*') if p.is_file() and p.suffix in {'.rs', '.ts', '.tsx', '.swift'})
    paths.extend(root / 'src-tauri' / name for name in ('tauri.conf.json', 'tauri.mas.conf.json', 'build.rs'))
    tables = {str(root / MAC_TABLE[0]): MAC_TABLE, str(root / IOS_TABLE[0]): IOS_TABLE}
    for path in paths:
        rel = path.relative_to(root)
        text = path.read_text()
        outside = table_block(root, tables[str(path)])[0] if str(path) in tables else text
        # Unit tests (iOS test target, Rust #[cfg(test)] modules, *.test.ts) may
        # name preset hosts in assertions; app code may not. Retired hosts never.
        tests = ''
        if path.suffix == '.rs' and '#[cfg(test)]' in outside:
            outside, tests = outside.split('#[cfg(test)]', 1)
        elif 'Tests' in str(rel) or path.name.endswith('.test.ts'):
            outside, tests = '', text
        for host in sorted(set(h.lower() for h in AI_HOSTS.findall(outside))):
            errors.append(f'AI service host {host} outside the preset table in {rel}')
        for host in sorted(set(h.lower() for h in RETIRED_HOSTS.findall(tests))):
            errors.append(f'Retired AI service host {host} in {rel}')
        if re.search(r'(?:env!|option_env!)\s*\(\s*"[^\"]*(?:API_KEY|TOKEN|SECRET)', text):
            errors.append(f'Compile-time credential injection in {rel}')
        for m in KEY_SHAPES.finditer(text):
            errors.append(f'Credential-shaped literal in {rel}:{text[:m.start()].count(chr(10)) + 1}')
        for pattern, kind in ((NAMED_LITERAL, 'named credential literal'), (AUTH_LITERAL, 'authorization literal')):
            for m in pattern.finditer(text):
                if not PLACEHOLDER.search(m.group(1)):
                    errors.append(f'{kind} in {rel}:{text[:m.start()].count(chr(10)) + 1}')

    # --- Bundled environment files; watch app ------------------------------
    for name in ('tauri.conf.json', 'tauri.mas.conf.json'):
        config = json.loads((root / 'src-tauri' / name).read_text())
        resources = config.get('bundle', {}).get('resources', [])
        values = list(resources.keys()) + list(resources.values()) if isinstance(resources, dict) else resources
        for value in values:
            need(not any(part.startswith('.env') for part in Path(value).parts), f'Environment file bundled by {name}')
            if any(c in value for c in '*?['):
                need(not any(p.name.startswith('.env') for p in (root / 'src-tauri').glob(value)), f'Resource glob includes an environment file in {name}')
    project = (root / 'ios/project.yml').read_text()
    need(not re.search(r'path:\s*[\"\']?[^\n]*\.env', project), 'iOS resources must not include an environment file')
    # The Apple Watch app records and hands audio to the iPhone only: no network AI or speech service in it
    for path in (root / 'ios/NoFrictionWatch').rglob('*.swift'):
        text = path.read_text()
        need(not re.search(r'\b(?:URLSession|URLRequest|NWConnection|SFSpeechRecognizer|SpeechAnalyzer)\b', text), f'Watch app must not make network or speech requests: {path.relative_to(root)}')
    # The screen-capture broadcast extension (docs/SCREEN_CAPTURE_IOS.md) sees
    # everything on screen: it writes to the App Group container and nothing
    # else. No network API of any kind, in its code or the code it shares.
    broadcast_files = [p for folder in ('ios/NoFrictionBroadcast', 'ios/ScreenCaptureShared')
                       for p in (root / folder).rglob('*.swift')]
    need(broadcast_files, 'Broadcast extension sources missing (ios/NoFrictionBroadcast)')
    for path in broadcast_files:
        text = path.read_text()
        need(not BROADCAST_NETWORK.search(text), f'Broadcast extension must not use network APIs: {path.relative_to(root)}')
    need(re.search(r'NoFrictionBroadcast:\s*\n\s*type:\s*app-extension', project), 'iOS project must define the NoFrictionBroadcast extension')
    ext_block = project.split('NoFrictionBroadcast:\n', 1)[-1].split('\n\n', 1)[0]
    need(not re.search(r'com\.apple\.developer\.networking|network', ext_block, re.I), 'Broadcast extension must not request network entitlements')
    return errors

def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    errors = audit(args.root.resolve())
    print(json.dumps({'status': 'FAIL' if errors else 'PASS',
                      'policy': 'Apple/custom AI with curated presets (OpenAI, Anthropic, Meta Muse, xAI, Mistral) as UI data only; '
                                'no keys, no default provider, no startup network, provider hosts only in the preset tables; local transcription',
                      'presets': list(CURATED), 'errors': errors}))
    return bool(errors)

if __name__ == '__main__':
    sys.exit(main())
