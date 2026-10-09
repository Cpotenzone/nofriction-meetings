#!/usr/bin/env python3
"""Redacted static release gate; keep alongside audit-embedded-credentials.py.

Exit 0: no findings in inspected scope; 1: credential candidate or enabled host-policy failure; 2: incomplete/error.
This is not proof that encrypted/fragmented/unknown-format secrets are absent.
For Tauri, require linked and decoded JS/HTML/CSS using --require-tauri-assets.
For the iPhone app, --require-embedded Watch/NoFrictionWatch.app=com.nofriction.meetings.watchkitapp
fails the scan as incomplete unless that nested bundle is present, has that identity and
its files were scanned (every file under the artifact is scanned either way).
"""
import argparse, datetime, hashlib, importlib.util, json, re, sys, tempfile
from pathlib import Path

sys.dont_write_bytecode = True
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('redacted_audit',HERE/'audit-embedded-credentials.py')
core=importlib.util.module_from_spec(spec);spec.loader.exec_module(core)
# Hosts that must never ship (removed cloud transcription and AI services).
RETIRED_SERVICE_HOSTS=re.compile(rb'\b(?:generativelanguage\.googleapis\.com|(?:[a-z0-9-]+-)?speech\.googleapis\.com|api\.deepgram\.com|api\.gladia\.io|api\.groq\.com|openrouter\.ai|api\.deepseek\.com|api\.perplexity\.ai|api\.together\.(?:xyz|ai))\b',re.I)
# Hosts of the curated provider presets (OpenAI, Anthropic, xAI, Mistral): allowed in
# the artifact as static preset data (docs/AI_PROVIDERS.md); inventoried, never a failure.
# The source guard (check-ai-provider-policy.py) proves they appear only in the preset tables.
PRESET_HOSTS=re.compile(rb'\b(?:api\.openai\.com|api\.anthropic\.com|api\.x\.ai|api\.mistral\.ai|platform\.openai\.com|platform\.claude\.com|console\.x\.ai|console\.mistral\.ai)\b',re.I)

def retired_hosts(name,data):
    return [{'file':name,'host':host.decode('ascii').lower(),'classification':'retired service host, not a credential'} for host in sorted(set(RETIRED_SERVICE_HOSTS.findall(data)))]

def preset_hosts(name,data):
    return [{'file':name,'host':host.decode('ascii').lower(),'classification':'curated preset host (static preset data), not a credential'} for host in sorted(set(PRESET_HOSTS.findall(data)))]

def run(args):
    path=Path(args.artifact).resolve()
    if not (path.is_dir() and path.suffix=='.app') and not (path.is_file() and path.suffix=='.ipa'):
        raise ValueError('artifact must be an existing .app directory or .ipa file')
    record=core.bundle('noFriction',path)
    if record.get('identity',{}).get('CFBundleIdentifier')!='com.nofriction.meetings':
        raise ValueError('bundle identity is not noFriction: refusing to classify another product')
    known={}
    if args.known_env_file:
        for line in Path(args.known_env_file).read_text().splitlines():
            entry=line.lstrip().lstrip('#').strip()
            if '=' not in entry:continue
            name,value=entry.split('=',1);value=value.strip().strip('\"\'')
            if value and re.fullmatch(r'[A-Z0-9_]+',name) and re.search(r'KEY|TOKEN|SECRET|PASSWORD|CONNECTION_STRING',name):
                known[name]=value.encode()
    exact=[];host_findings=[];preset_findings=[]
    if path.suffix=='.app':
        members=((str(p.relative_to(path)),p.read_bytes()) for p in path.rglob('*') if p.is_file())
        app_prefix=''
    else:
        import zipfile
        z=zipfile.ZipFile(path)
        members=((n,z.read(n)) for n in z.namelist() if n.startswith('Payload/') and not n.endswith('/'))
        app_prefix=str(Path(record.get('executable_file','Payload/x.app/x')).parent)+'/'
    # Nested bundles that must be present and scanned (e.g. the Apple Watch app)
    wanted={}
    for spec in getattr(args,'require_embedded',None) or []:
        rel,_,bundle_id=spec.partition('=')
        wanted[app_prefix+rel.strip('/')+'/']={'path':rel.strip('/'),'expected_identifier':bundle_id,'identifier':None,'files_scanned':0,'present':False}
    executable=None
    for name,data in members:
        for prefix,info in wanted.items():
            if name.startswith(prefix):
                info['files_scanned']+=1
                if name==prefix+'Info.plist':
                    import plistlib
                    try:
                        plist=plistlib.loads(data)
                        info['present']=True
                        info['identifier']=plist.get('CFBundleIdentifier')
                        info['version']=plist.get('CFBundleShortVersionString')
                        info['build']=plist.get('CFBundleVersion')
                    except Exception:
                        info['present']=False
        if name==record.get('executable_file'):executable=data
        host_findings.extend(retired_hosts(name,data))
        preset_findings.extend(preset_hosts(name,data))
        for key,value in known.items():
            if value in data:exact.append({'file':name,'local_variable_name':key,'value':'REDACTED'})
    assets=[];asset_matches=[];embedded_types=set()
    if args.asset_cache:
        try:import brotli
        except ImportError:raise ValueError('Brotli decoder unavailable; cannot inspect Tauri assets')
        seen=set()
        for folder in args.asset_cache:
            root=Path(folder).resolve()
            if not root.is_dir():raise ValueError('asset cache directory missing')
            for p in sorted(root.rglob('*')):
                if not p.is_file() or p.suffix not in ('.js','.html','.css','.svg','.json'):continue
                raw=p.read_bytes();fingerprint=core.sha(raw)
                if fingerprint in seen:continue
                seen.add(fingerprint)
                linked=bool(executable and raw in executable)
                try:data=brotli.decompress(raw);compression='brotli'
                except Exception:data=raw;compression='none'
                hits=core.scan(str(p),data,True)
                local=[k for k,v in known.items() if v in data]
                assets.append({'file':str(p),'compressed_sha256':fingerprint,'decoded_sha256':core.sha(data),'compression':compression,'bytes':len(data),'exact_blob_present_in_executable':linked,'redacted_matches':hits,'exact_local_variable_matches':local})
                # Old unrelated cache blobs cannot fail a new artifact; they remain inventoried.
                if linked:
                    embedded_types.add(p.suffix)
                    asset_matches.extend(hits)
                    host_findings.extend(retired_hosts(str(p),data))
                    preset_findings.extend(preset_hosts(str(p),data))
                    exact.extend({'file':str(p),'local_variable_name':k,'value':'REDACTED'} for k in local)
    embedded=list(wanted.values())
    embedded_ok=all(e['present'] and e['files_scanned']>1 and (not e['expected_identifier'] or e['identifier']==e['expected_identifier']) for e in embedded)
    coverage_ok=(not args.require_tauri_assets or {'.js','.html','.css'}.issubset(embedded_types)) and embedded_ok
    count=len(record['redacted_matches'])+len(asset_matches)+len(exact)
    forbidden_hosts=bool(args.reject_retired_services and host_findings)
    status='INCOMPLETE' if not coverage_ok else 'FAIL_CANDIDATES' if count else 'FAIL_RETIRED_SERVICES' if forbidden_hosts else 'PASS_STATIC_SCOPE'
    output={'schema':'nofriction.redacted-release-credential-gate.v1','checked_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'status':status,'candidate_count':count,'reject_retired_services':args.reject_retired_services,'retired_service_host_findings':host_findings,'preset_host_findings':preset_findings,'embedded_bundles':embedded,'artifact':record,'decoded_assets':assets,'known_local_variable_names':sorted(known),'exact_local_matches':exact,'tauri_linked_types':sorted(embedded_types),'limitations':['Static pass is not proof of zero credentials: opaque, encrypted, fragmented or unrecognised secrets may escape patterns.','No credential validity requests are sent. A match is a candidate, not proof of an active credential.','For Tauri, raw executable scans do not cover compressed content unless linked assets were decoded.','Retired-host matches establish packaged strings, not execution; unknown, constructed or encrypted hosts can evade this list.','Preset hosts (OpenAI, Anthropic, xAI, Mistral) are expected static preset data and are inventoried, not failed; the source guard proves they appear only in the preset tables.','Independent credential-loading, build-environment and source-to-artifact audit remains required.']}
    Path(args.receipt).write_text(json.dumps(output,indent=2)+'\n')
    print(json.dumps({'status':status,'candidate_count':count,'retired_service_host_count':len(host_findings),'preset_host_count':len(preset_findings),'reject_retired_services':args.reject_retired_services,'receipt':str(Path(args.receipt).resolve()),'bundle_identity':record['identity'],'executable_sha256':record.get('executable_sha256'),'tauri_linked_types':sorted(embedded_types),'embedded_bundles':[{k:e.get(k) for k in ('path','identifier','files_scanned','present')} for e in embedded]}))
    return 2 if not coverage_ok else 1 if count or forbidden_hosts else 0

def self_test():
    import plistlib, zipfile
    with tempfile.TemporaryDirectory(prefix='nf-secret-scan-test-') as folder:
        root=Path(folder);app=root/'Test.app';app.mkdir()
        (app/'Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'com.nofriction.meetings','CFBundleExecutable':'Test','CFBundleVersion':'fixture','CFBundleShortVersionString':'fixture'}))
        (app/'.env').write_bytes(b'EXAMPLE_TOKEN=\n# unrelated comment must not become a value\n')
        args=argparse.Namespace(artifact=str(app),receipt=str(root/'receipt.json'),known_env_file=None,asset_cache=[],require_tauri_assets=False,reject_retired_services=False,require_embedded=[])
        (app/'Test').write_bytes(b'ordinary non-secret fixture')
        assert run(args)==0
        candidate=b'sk-'+b'notavalidcredential1234567890'
        (app/'Test').write_bytes(candidate)
        assert run(args)==1
        assert candidate.decode() not in (root/'receipt.json').read_text()
        (app/'Test').write_bytes(b'ordinary non-secret fixture')
        args.require_tauri_assets=True
        assert run(args)==2
        args.require_tauri_assets=False
        ipa=root/'noFriction.ipa'
        with zipfile.ZipFile(ipa,'w') as z:
            for p in app.rglob('*'):
                if p.is_file():z.write(p,'Payload/noFriction.app/'+str(p.relative_to(app)))
            # An extension's Info.plist must not replace the main app identity.
            z.writestr('Payload/noFriction.app/PlugIns/Extension.appex/Info.plist',plistlib.dumps({'CFBundleIdentifier':'com.example.extension','CFBundleExecutable':'Extension'}))
        args.artifact=str(ipa)
        assert run(args)==0
        result=json.loads((root/'receipt.json').read_text())
        assert result['artifact']['executable_file']=='Payload/noFriction.app/Test'
        assert result['artifact']['identity']['CFBundleIdentifier']=='com.nofriction.meetings'
        args.artifact=str(app)
        (app/'.env').write_bytes(b'EXAMPLE_API_KEY='+b'opaque_synthetic_not_a_real_key\n')
        assert run(args)==1
        (app/'.env').write_bytes(b'EXAMPLE_TOKEN=\n')
        (app/'Test').write_bytes(b'https://api.deepgram.com/v1')
        args.reject_retired_services=True
        assert run(args)==1
        assert json.loads((root/'receipt.json').read_text())['status']=='FAIL_RETIRED_SERVICES'
        # Curated preset hosts are static preset data: inventoried, never a failure
        (app/'Test').write_bytes(b'https://api.openai.com/v1 https://api.anthropic.com/v1 https://api.x.ai/v1 https://api.mistral.ai/v1')
        assert run(args)==0, 'preset hosts must not fail the artifact scan'
        assert json.loads((root/'receipt.json').read_text())['preset_host_findings'][0]['host']=='api.anthropic.com'
        # Embedded Apple Watch app: required, identified and scanned
        (app/'Test').write_bytes(b'ordinary non-secret fixture')
        args.require_embedded=['Watch/NoFrictionWatch.app=com.nofriction.meetings.watchkitapp']
        assert run(args)==2, 'missing watch bundle must be incomplete'
        watch=app/'Watch'/'NoFrictionWatch.app';watch.mkdir(parents=True)
        (watch/'Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'com.example.other','CFBundleExecutable':'NoFrictionWatch'}))
        (watch/'NoFrictionWatch').write_bytes(b'watch fixture')
        assert run(args)==2, 'wrong watch identity must be incomplete'
        (watch/'Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'com.nofriction.meetings.watchkitapp','CFBundleExecutable':'NoFrictionWatch'}))
        assert run(args)==0
        receipt=json.loads((root/'receipt.json').read_text())
        assert receipt['artifact']['identity']['CFBundleIdentifier']=='com.nofriction.meetings', 'nested plist must not replace the app identity'
        assert receipt['embedded_bundles'][0]['files_scanned']==2
        (watch/'NoFrictionWatch').write_bytes(b'https://api.deepgram.com/v1')
        assert run(args)==1, 'retired host inside the watch app must fail'
        (watch/'NoFrictionWatch').write_bytes(b'sk-'+b'notavalidcredential1234567890')
        assert run(args)==1, 'credential candidate inside the watch app must fail'
        (watch/'NoFrictionWatch').write_bytes(b'watch fixture')
        wipa=root/'watch.ipa'
        with zipfile.ZipFile(wipa,'w') as z:
            for p in app.rglob('*'):
                if p.is_file():z.write(p,'Payload/noFriction.app/'+str(p.relative_to(app)))
        args.artifact=str(wipa)
        assert run(args)==0
        assert json.loads((root/'receipt.json').read_text())['embedded_bundles'][0]['identifier']=='com.nofriction.meetings.watchkitapp'
        args.artifact=str(app);args.require_embedded=[]
        import brotli
        cache=root/'assets';cache.mkdir()
        blobs=[]
        for ext,body in (('.js',b'fetch("https://api.deepgram.com/v1/listen");'),('.html',b'<html>fixture</html>'),('.css',b'body { color: black; }')):
            blob=brotli.compress(body);blobs.append(blob);(cache/('fixture'+ext)).write_bytes(blob)
        (app/'Test').write_bytes(b'fixture\0'+b'\0'.join(blobs))
        args.asset_cache=[str(cache)];args.require_tauri_assets=True
        assert run(args)==1
        assert json.loads((root/'receipt.json').read_text())['status']=='FAIL_RETIRED_SERVICES'
    print(json.dumps({'self_test':'PASS','cases':14,'secret_values_printed':False}))
    return 0

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact');parser.add_argument('--receipt')
    parser.add_argument('--asset-cache',action='append',default=[])
    parser.add_argument('--require-tauri-assets',action='store_true')
    parser.add_argument('--reject-retired-services',action='store_true',help='fail if named retired service hosts remain in raw or linked decoded artifact content')
    parser.add_argument('--known-env-file')
    parser.add_argument('--require-embedded',action='append',default=[],metavar='PATH=BUNDLE_ID',help='nested bundle (relative to the .app) that must be present with this identity and scanned')
    parser.add_argument('--self-test',action='store_true')
    args=parser.parse_args()
    try:
        if args.self_test:code=self_test()
        elif not args.artifact or not args.receipt:parser.error('--artifact and --receipt are required')
        else:code=run(args)
    except Exception as exc:
        # Avoid echoing exception detail: parser/decode errors can include data fragments.
        print(json.dumps({'status':'INCOMPLETE','error_type':type(exc).__name__,'message':'Scan failed; inspect paths, artifact identity, decoder and readable-file permissions.'}))
        code=2
    sys.exit(code)
