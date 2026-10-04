#!/usr/bin/env python3
"""Local-only, redacted static receipt. Never outputs candidate credential values."""
from pathlib import Path
import datetime, hashlib, json, math, os, plistlib, re, zipfile

HERE = Path(__file__).resolve().parent
PATTERNS = {
    'OpenAI_or_Anthropic_secret_shape': rb'\bsk-(?:proj-|svcacct-|ant-)?[A-Za-z0-9_-]{20,}',
    'Google_API_key_shape_public_or_private': rb'\bAIza[A-Za-z0-9_-]{35}\b',
    'Groq_secret_shape': rb'\bgsk_[A-Za-z0-9]{20,}',
    'xAI_secret_shape': rb'\bxai[-_][A-Za-z0-9_-]{20,}',
    'Perplexity_secret_shape': rb'\bpplx-[A-Za-z0-9_-]{20,}',
    'Pinecone_secret_shape': rb'\bpcsk_[A-Za-z0-9_-]{20,}',
    'HuggingFace_token_shape': rb'\bhf_[A-Za-z0-9]{20,}',
    'AWS_access_key_shape': rb'\b(?:AKIA|ASIA)[A-Z0-9]{16}\b',
    'complete_private_key_pem': rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\s+[A-Za-z0-9+/=\r\n]{50,}-----END (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
    'jwt_shape': rb'\beyJ[A-Za-z0-9_-]{12,}\.[A-Za-z0-9_-]{12,}\.[A-Za-z0-9_-]{20,}',
}
ASSIGNMENT = re.compile(rb'''(?i)(?<![A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]{0,96}(?:api_?key|secret|access_?token|auth_?token|client_?token|password|credential)[A-Za-z0-9_]{0,96}|api_?key|secret|token|password|credential)["']?\s*[:=]\s*(?:Some\()?\s*["']([^"'\r\n]{12,4096})["']''')
AUTH_LITERAL = re.compile(rb'''(?i)(Authorization|x-api-key)["']?\s*[,=:]\s*["']((?:Bearer |Token |Basic )?[A-Za-z0-9_+./=-]{16,})["']''')
ENV_LITERAL = re.compile(rb'''(?m)^[ \t]*#?[ \t]*(?:export[ \t]+)?([A-Z][A-Z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD|CONNECTION_STRING))[ \t]*=[ \t]*["']?([^\r\n"']{12,4096})''')
PLIST_SENSITIVE = re.compile(r'api.?key|client.?token|secret|access.?token|password', re.I)
SKIP = {'.git','node_modules','target','build','.build','.local','.venv','venv','DerivedData','vendor','Pods','Carthage','AppStore','__pycache__'}
EXT = {'.swift','.rs','.m','.mm','.h','.c','.cpp','.tsx','.ts','.jsx','.js','.json','.plist','.xcconfig','.pbxproj','.yml','.yaml','.toml','.sh','.env','.entitlements','.storekit'}

def sha(data): return hashlib.sha256(data).hexdigest()
def placeholder(value):
    v=value.lower()
    return any(t in v for t in (b'placeholder',b'example',b'changeme',b'fake',b'test',b'dummy',b'your_',b'your-',b'<',b'...')) or len(set(v)) < 5

def scan(name, data, text_source=False):
    hits=[]
    for kind,pattern in PATTERNS.items():
        for m in re.finditer(pattern,data):
            h={'file':name,'detector':kind,'byte_offset':m.start(),'value':'REDACTED','placeholder_like':placeholder(m.group())}
            if text_source: h['line']=data[:m.start()].count(b'\n')+1
            hits.append(h)
    if text_source or name.endswith(('.json','.js','.plist','.env','.html','.xml','.toml','.yml','.yaml')):
        for pattern,kind in ((ASSIGNMENT,'credential_named_literal'),(AUTH_LITERAL,'authorization_literal')):
            for m in pattern.finditer(data):
                hits.append({'file':name,'detector':kind,'field_name':m.group(1).decode('ascii','replace'),'line':data[:m.start()].count(b'\n')+1,'value':'REDACTED','placeholder_like':placeholder(m.group(2)),'literal_bytes':len(m.group(2))})
    if Path(name).name.startswith('.env'):
        for m in ENV_LITERAL.finditer(data):
            hits.append({'file':name,'detector':'credential_named_env_assignment','field_name':m.group(1).decode('ascii','replace'),'line':data[:m.start()].count(b'\n')+1,'value':'REDACTED','placeholder_like':placeholder(m.group(2))})
    if name.endswith('.plist'):
        try: parsed=plistlib.loads(data)
        except Exception: parsed=None
        def walk(obj,path=''):
            if isinstance(obj,dict):
                for k,v in obj.items():
                    kp=path+'/'+str(k)
                    if PLIST_SENSITIVE.search(str(k)):
                        hits.append({'file':name,'detector':'plist_credential_named_field','field_name':kp,'nonempty':bool(v),'value':'REDACTED','placeholder_like':isinstance(v,str) and placeholder(v.encode())})
                    walk(v,kp)
            elif isinstance(obj,list):
                for i,v in enumerate(obj): walk(v,path+'/'+str(i))
        walk(parsed)
    return hits

def files_under(root):
    if root.is_file(): yield root; return
    for base,dirs,files in os.walk(root):
        dirs[:]=[d for d in dirs if d not in SKIP and not d.endswith(('.app','.xcarchive','.framework'))]
        for f in files:
            p=Path(base)/f
            if p.suffix in EXT or p.name.startswith('.env'): yield p

def source(name,root,include):
    files=sorted(set(p for path in include for p in files_under(root/path) if p.is_file()))
    manifest=[];hits=[]
    for p in files:
        data=p.read_bytes(); rel=str(p.relative_to(root))
        manifest.append({'file':rel,'bytes':len(data),'sha256':sha(data)})
        hits.extend(scan(rel,data,True))
    return {'product':name,'scope':'source','root':str(root),'include':include,'file_count':len(files),'source_manifest_sha256':sha(json.dumps(manifest,sort_keys=True).encode()),'files':manifest,'redacted_matches':hits}

def bundle(name,path):
    entries=[]
    if path.suffix=='.ipa':
        with zipfile.ZipFile(path) as z:
            entries=[(n,z.read(n)) for n in z.namelist() if n.startswith('Payload/') and not n.endswith('/')]
        receipt={'artifact_sha256':sha(path.read_bytes()),'artifact_bytes':path.stat().st_size}
    else:
        entries=[(str(p.relative_to(path)),p.read_bytes()) for p in sorted(path.rglob('*')) if p.is_file()]
        receipt={}
    matches=[];manifest=[];identity=None;executable=None
    for n,b in entries:
        manifest.append({'file':n,'bytes':len(b),'sha256':sha(b)})
        matches.extend(scan(n,b))
        if n in ('Info.plist','Contents/Info.plist') or re.fullmatch(r'Payload/[^/]+\.app/Info\.plist',n):
            info=plistlib.loads(b)
            identity={k:info.get(k) for k in ('CFBundleIdentifier','CFBundleShortVersionString','CFBundleVersion')}
            executable=str(Path(n).parent/('MacOS' if n=='Contents/Info.plist' else '')/info['CFBundleExecutable'])
    for n,b in entries:
        if n==executable:
            receipt['executable_file']=n;receipt['executable_sha256']=sha(b)
    receipt.update({'product':name,'scope':'artifact','path':str(path),'mtime_utc':datetime.datetime.fromtimestamp(path.stat().st_mtime,datetime.timezone.utc).isoformat(),'identity':identity,'file_count':len(entries),'bundle_manifest_sha256':sha(json.dumps(manifest,sort_keys=True).encode()),'files':manifest,'redacted_matches':matches})
    return receipt
