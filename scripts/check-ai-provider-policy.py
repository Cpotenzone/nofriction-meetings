#!/usr/bin/env python3
"""Release source policy: noFriction has Apple on-device and user-entered AI only.

Run before release builds. Model-download URLs are intentionally allowed; they
install local transcription models and are not hosted AI inference endpoints.
Runtime request/legacy-settings tests remain the behavioral verification.
"""
import argparse, json, re, sys
from pathlib import Path

RETIRED = re.compile(r'(?:api\.(?:openai\.com|anthropic\.com|deepgram\.com|gladia\.io|groq\.com|x\.ai|mistral\.ai|deepseek\.com|perplexity\.ai|together\.xyz|together\.ai)|generativelanguage\.googleapis\.com|speech\.googleapis\.com|openrouter\.ai)', re.I)

def audit(root):
    errors=[]
    def need(condition, message):
        if not condition: errors.append(message)
    rust=(root/'src-tauri/src/ai/providers.rs').read_text()
    block=rust.split('pub static PRESETS: &[Preset] = &[',1)[1].split('\n];',1)[0]
    ids=re.findall(r'\bid:\s*([^,]+),',block)
    need(ids==['APPLE_PROVIDER','"custom"'], 'Mac AI provider table must contain only Apple and custom')
    custom=block.split('id: "custom",',1)[-1]
    need(re.search(r'base_url:\s*""\s*,',custom), 'Mac custom endpoint must start empty')
    need(not re.search(r'https?://|\bkey_prefixes:\s*&\[\s*"|\bkey_url:\s*"[^\"]',block), 'Provider presets must not contain network URLs or credential routing')
    swift=(root/'ios/NoFriction/AI/AIProvider.swift').read_text()
    table=re.search(r'static let all:\s*\[AIProvider\]\s*=\s*\[([^\]]+)\]',swift)
    need(table and set(re.findall(r'\.(\w+)',table.group(1)))=={'custom','apple'}, 'iOS AI provider table must contain only Apple and custom')
    need(set(re.findall(r'AIProvider\(id:\s*"([^\"]+)"',swift))=={'custom','apple'}, 'iOS must define no named provider presets')
    trans=(root/'src-tauri/src/transcription/mod.rs').read_text()
    enum=re.search(r'pub enum ProviderType\s*\{([^}]+)\}',trans).group(1)
    enum=re.sub(r'//[^\n]*','',enum)
    need(re.findall(r'\b[A-Z]\w*\b',enum)==['Local'], 'Mac transcription must expose only local execution')
    need(not re.search(r'pub mod (deepgram|gemini|gladia|google_stt)\s*;',trans), 'Retired cloud transcription modules must not compile')
    paths=[]
    for folder in ('src','src-tauri/src','ios/NoFriction'):
        paths.extend(p for p in (root/folder).rglob('*') if p.is_file() and p.suffix in {'.rs','.ts','.tsx','.swift'})
    paths.extend(root/'src-tauri'/name for name in ('tauri.conf.json','tauri.mas.conf.json','build.rs'))
    for path in paths:
        text=path.read_text()
        if RETIRED.search(text):errors.append(f'Retired AI service host in {path.relative_to(root)}')
        if re.search(r'(?:env!|option_env!)\s*\(\s*"[^\"]*(?:API_KEY|TOKEN|SECRET)',text):errors.append(f'Compile-time credential injection in {path.relative_to(root)}')
    for name in ('tauri.conf.json','tauri.mas.conf.json'):
        config=json.loads((root/'src-tauri'/name).read_text())
        resources=config.get('bundle',{}).get('resources',[])
        values=list(resources.keys())+list(resources.values()) if isinstance(resources,dict) else resources
        for value in values:
            need(not any(part.startswith('.env') for part in Path(value).parts), f'Environment file bundled by {name}')
            if any(c in value for c in '*?['):
                need(not any(p.name.startswith('.env') for p in (root/'src-tauri').glob(value)),f'Resource glob includes an environment file in {name}')
    project=(root/'ios/project.yml').read_text()
    need(not re.search(r'path:\s*[\"\']?[^\n]*\.env',project), 'iOS resources must not include an environment file')
    return errors

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    args=parser.parse_args()
    errors=audit(args.root.resolve())
    print(json.dumps({'status':'FAIL' if errors else 'PASS','policy':'Apple/custom AI; local transcription; no service defaults or bundled environment files','errors':errors}))
    return bool(errors)

if __name__=='__main__':sys.exit(main())
