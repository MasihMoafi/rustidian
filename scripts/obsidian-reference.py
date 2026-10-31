"""Capture the installed Obsidian using a disposable profile and sample vault."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from PIL import Image, ImageGrab
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--section", choices=["all", "ux", "callouts", "navigation", "hover", "menus", "inventory", "workspaces", "recovery", "recovery-live", "note-creators", "unique-settings", "unique-links", "unique-links-configured", "tab-actions", "tab-windows", "file-actions", "file-move", "page-preview", "page-preview-actions", "native-highlights", "native-highlight-caret", "native-highlight-render", "native-highlight-availability", "native-highlight-empty", "empty-commands"], default="all")
parser.add_argument("--appimage", type=Path, default=Path("/home/masih/Desktop/o/Obsidian.AppImage"))
parser.add_argument("--app-asar", type=Path, help="Read-only copy of an installed update into the disposable profile")
arguments = parser.parse_args()
with tempfile.TemporaryDirectory(prefix="rustidian-reference-") as temporary:
    base = Path(temporary)
    vault = base / "Reference Vault"
    shutil.copytree(ROOT / "demo-vault", vault)
    if arguments.section == "navigation":
        (vault / "Notes/Navigation check.md").write_text("# Navigation check\n\n- [ ] Open task\n- [x] Completed task\n  - [ ] Nested task\n\n[Open target](Linked%20target.md#Section)\n\n[[Notes/Linked target|Wiki target]]\n")
        (vault / "Notes/Linked target.md").write_text("# Linked target\n\n## Section\n\nDestination text.\n")
    settings = vault / ".obsidian"
    settings.mkdir(exist_ok=True)
    original = Path("/home/masih/Desktop/o/Main Vault/.obsidian")
    appearance = json.loads((original / "appearance.json").read_text())
    appearance["theme"] = "obsidian"
    (settings / "appearance.json").write_text(json.dumps(appearance))
    shutil.copytree(original / "snippets", settings / "snippets", dirs_exist_ok=True)
    (settings / "app.json").write_text(json.dumps({"defaultViewMode": "preview"}))
    if arguments.section in ("inventory", "workspaces", "recovery", "recovery-live", "note-creators", "unique-settings", "unique-links", "unique-links-configured", "tab-actions", "tab-windows", "page-preview", "page-preview-actions"):
        (vault / "Templates").mkdir(exist_ok=True)
        (vault / "Daily").mkdir(exist_ok=True)
        (vault / "Template target.md").write_text("# Before\n\n")
        (vault / "Templates/Daily.md").write_text("# {{title}}\n\n{{date:YYYY-MM-DD}} {{time:HH:mm}}\n\n- [ ] Review\n")
        (vault / "Templates/Meeting.md").write_text("# Meeting {{title}}\n\nOn {{date:YYYY-MM-DD}}\n\n{{content}}\n")
        (settings / "templates.json").write_text(json.dumps({"folder": "Templates", "dateFormat": "YYYY-MM-DD", "timeFormat": "HH:mm"}))
        (settings / "daily-notes.json").write_text(json.dumps({"folder": "Daily", "template": "Templates/Daily", "format": "YYYY-MM-DD", "autorun": False}))
        plugins = "file-explorer global-search switcher graph backlink canvas outgoing-link tag-pane properties page-preview daily-notes templates note-composer command-palette slash-command editor-status bookmarks zk-prefixer random-note outline word-count slides workspaces file-recovery footnotes bases"
        (settings / "core-plugins.json").write_text(json.dumps({name: True for name in plugins.split()}))
        if arguments.section == "unique-links-configured":
            (vault / "Unique").mkdir()
            (settings / "zk-prefixer.json").write_text(json.dumps({"folder":"Unique","format":"[Link-]YYYYMMDDHHmm","template":"Templates/Meeting.md"}))
        if arguments.section in ("page-preview", "page-preview-actions"):
            (vault/'Preview source.md').write_text('# Preview source\n\n[[Preview target|Target preview]]\n')
            (vault/'Preview target.md').write_text('# Preview target\n\nVISIBLE_PREVIEW_DESTINATION\n\n- [ ] Preview task\n')
            if arguments.section == "page-preview-actions":
                (vault/'Preview target.md').write_text('# Preview target\n\nVISIBLE_PREVIEW_DESTINATION\n\n- [ ] Preview task\n\n[Open next](Preview%20next.md)\n')
                (vault/'Preview next.md').write_text('# Preview next\n\nNEXT_PREVIEW_DESTINATION\n')
                (vault/'Preview source.md').write_text('# Preview source\n\n[[Preview target|Target preview]]\n\n[[Missing preview target|Missing preview]]\n')
    if arguments.section in ("native-highlights", "native-highlight-caret", "native-highlight-render", "native-highlight-availability", "native-highlight-empty"):
        (vault / "Native highlights.md").write_text("# Native highlights\n\n`==literal==`\n\nBefore TARGET")
        if arguments.section == "native-highlight-empty":
            (vault/"Native highlights.md").write_text("Before  ==🟣EXISTING== after")
        if arguments.section == "native-highlight-render":
            (vault/"Native highlight rendering.md").write_text("# Native highlight rendering\n\n"+"\n\n".join(f"{name} =={marker}TARGET_{name.upper()}==" for name,marker in [('Blue','🔵'),('Green','🟢'),('Orange','🟠'),('Purple','🟣'),('Red','🔴'),('Yellow','🟡')])+"\n\nPartial ==🔵TA====🟣RGE====🔵T==\n\n`==🔵literal==`\n\n```md\n==🟣fenced==\n```\n\n[==🔴target link==](Native%20highlights.md)\n")
    profile = base / "profile"
    profile.mkdir()
    update_identity = {}
    if arguments.app_asar:
        metadata = arguments.app_asar.lstat()
        assert arguments.app_asar.is_file() and not arguments.app_asar.is_symlink() and metadata.st_size <= 64*1024*1024
        update_bytes = arguments.app_asar.read_bytes()
        (profile / arguments.app_asar.name).write_bytes(update_bytes)
        update_identity = {"app_asar": str(arguments.app_asar), "app_asar_sha256": hashlib.sha256(update_bytes).hexdigest()}
    (profile / "obsidian.json").write_text(json.dumps({"updateDisabled": True, "vaults": {"0123456789abcdef": {"path": str(vault), "ts": 1, "open": True}}}))
    read_fd, write_fd = os.pipe()
    xvfb_log = (ROOT / 'reports/reference-xvfb.log').open('w')
    server = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1440x960x24", "-nolisten", "tcp", "-ac", "-noreset"], pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=xvfb_log)
    os.close(write_fd)
    with os.fdopen(read_fd) as display_file:
        display = ":" + display_file.readline().strip()
    environment = {**os.environ, "DISPLAY": display, "WAYLAND_DISPLAY": "", "ELECTRON_OZONE_PLATFORM_HINT": "x11", "GDK_BACKEND": "x11", "GTK_USE_PORTAL": "0", "DBUS_SESSION_BUS_ADDRESS": f"unix:path=/tmp/rustidian-reference-no-bus-{os.getpid()}"}
    for _ in range(50):
        ready = subprocess.run(["xdotool", "getdisplaygeometry"], env=environment,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if ready.returncode == 0:
            break
        time.sleep(.1)
    else:
        server.terminate()
        server.wait(timeout=5)
        raise RuntimeError(f"Private reference X server {display} did not accept a connection")
    log = (ROOT / "reports/obsidian-reference.log").open("w")
    with socket.socket() as available:
        available.bind(("127.0.0.1", 0))
        debug_port = available.getsockname()[1]
    app = subprocess.Popen([str(arguments.appimage), "--no-sandbox", "--disable-gpu", "--in-process-gpu", "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE 127.0.0.1", f"--user-data-dir={profile}", "--ozone-platform=x11", f"--remote-debugging-port={debug_port}"], env=environment, stdout=log, stderr=log, start_new_session=True)
    def xdo(*arguments):
        return subprocess.check_output(["xdotool", *arguments], env=environment, text=True, stderr=subprocess.DEVNULL).strip()
    try:
        deadline = time.monotonic() + 30
        window = None
        while time.monotonic() < deadline:
            try:
                window = xdo("search", "--onlyvisible", "--class", "obsidian").splitlines()[0]
                if "Reference Vault" in xdo("getwindowname", window):
                    break
            except (subprocess.CalledProcessError, IndexError):
                pass
            time.sleep(0.2)
        assert window, "No isolated Obsidian window appeared"
        xdo("windowsize", window, "1440", "960")
        xdo("windowmove", window, "0", "0")
        xdo("windowfocus", window)
        time.sleep(2)
        xdo("key", "--clearmodifiers", "ctrl+o")
        time.sleep(0.3)
        xdo("type", "--clearmodifiers", "Home")
        time.sleep(0.3)
        xdo("key", "Return")
        time.sleep(1)
        if arguments.section != "callouts":
            ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-reference.png")
        with urllib.request.urlopen(f"http://127.0.0.1:{debug_port}/json/list") as response:
            targets = json.load(response)
        target = next(target for target in targets if target["type"] == "page")
        query = "JSON.stringify(['.markdown-preview-view','.inline-title','h1','h2','.markdown-preview-view p','.markdown-preview-view li','.markdown-preview-view strong','.markdown-preview-view ul','.markdown-preview-view blockquote','.markdown-preview-sizer','.view-header','.workspace-tab-header-container','.nav-header','.nav-files-container','.status-bar','.nav-file-title','.workspace-tab-header'].map(selector => {const e=document.querySelector(selector);if(!e)return {selector};const s=getComputedStyle(e);return {selector,font:s.fontFamily,size:s.fontSize,weight:s.fontWeight,lineHeight:s.lineHeight,marginTop:s.marginTop,marginBottom:s.marginBottom,rect:{x:e.getBoundingClientRect().x,y:e.getBoundingClientRect().y,width:e.getBoundingClientRect().width,height:e.getBoundingClientRect().height},padding:[s.paddingTop,s.paddingRight,s.paddingBottom,s.paddingLeft],border:s.borderBottomWidth};}))"
        code = "const ws=new WebSocket(process.argv[1]);ws.onopen=()=>ws.send(JSON.stringify({id:1,method:'Runtime.evaluate',params:{expression:process.argv[2],returnByValue:true}}));ws.onmessage=(e)=>{const r=JSON.parse(e.data);if(r.id===1){console.log(r.result.result.value);ws.close();}};setTimeout(()=>process.exit(1),5000).unref();"
        def evaluate(expression):
            value = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], expression], text=True, timeout=8)
            return json.loads(value)

        reference_identity = {"appimage": str(arguments.appimage), **update_identity}
        if arguments.app_asar:
            xdo("key", "--clearmodifiers", "ctrl+comma");time.sleep(.5)
            main_target=target
            opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def pages():
                with opener.open(f"http://127.0.0.1:{debug_port}/json/list",timeout=2) as response:return json.load(response)
            deadline=time.monotonic()+5
            settings_target=None
            while time.monotonic()<deadline:
                settings_target=next((page for page in pages() if page['type']=='page' and page['id']!=main_target['id']),None)
                if settings_target:break
                time.sleep(.1)
            assert settings_target, "The actual separate settings window must appear"
            target=settings_target
            general=evaluate("JSON.stringify((()=>{const e=[...document.querySelectorAll('.vertical-tab-nav-item')].find(e=>e.textContent.trim()==='General');if(!e)return null;const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
            assert general, "General settings must be present in the actual reference"
            xdo("mousemove",*[str(round(v)) for v in general],"click","1");time.sleep(.4)
            general_text=evaluate("JSON.stringify(document.querySelector('.modal.mod-settings')?.innerText||'')")
            version=re.search(r"(?:current\s+)?version\s*:?\s*(\d+\.\d+\.\d+)",general_text,re.I)
            reference_identity.update(observed_general_version=version.group(1) if version else None,general_text=general_text)
            ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-reference-1.14.4-general.png')
            xdo("key","Escape");time.sleep(.2)
            if any(page['id']==settings_target['id'] for page in pages()):
                xdo("key","--clearmodifiers","ctrl+w");time.sleep(.3)
            assert all(page['id']!=settings_target['id'] for page in pages()), "Close the isolated settings window before testing notes"
            target=main_target;xdo("windowfocus",window)
            print(json.dumps({"reference_identity":{k:v for k,v in reference_identity.items() if k!="general_text"}}),flush=True)
            assert reference_identity['observed_general_version']==re.search(r"(\d+\.\d+\.\d+)",arguments.app_asar.name).group(1),reference_identity

        if arguments.section == "ux":
            output = ROOT / 'reports/reference-ux'
            output.mkdir(exist_ok=True)
            source = '# Resize tables\n\n| Item | Value |\n| --- | --- |\n| Short | A longer value that must remain readable |\n\n> | Nested | Value |\n> | --- | --- |\n> | Quote | Safe |\n'
            note = vault / 'Notes/Resize tables.md'
            note.write_text(source)
            for name in ('Alpha', 'Beta', 'Gamma', 'Delta'):
                (vault / f'Drag {name}.md').write_text(f'# Drag {name}\n\nKeep {name}.\n')
            time.sleep(.8)
            report = {**reference_identity, 'window': [1440, 960], 'fixture': source, 'status': 'observed', 'actions': []}
            def show_table(mode):
                evaluate('(app.workspace.activeLeaf.setViewState({type:"markdown",state:{file:"Notes/Resize tables.md",mode:'+json.dumps(mode)+',source:false}}),JSON.stringify("opening"))')
                time.sleep(.8)
            def table_bounds():
                return evaluate("JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active table')].filter(table=>table.getBoundingClientRect().width>0).map(table=>({rect:(()=>{const r=table.getBoundingClientRect();return [r.x,r.y,r.width,r.height]})(),columns:[...table.rows[0].cells].map(cell=>{const r=cell.getBoundingClientRect();return [r.x,r.y,r.width,r.height]})})))")
            def move(start, end):
                xdo('mousemove','--sync',str(round(start[0])),str(round(start[1])),'mousedown','1')
                try:
                    for step in range(1,21):
                        point=[round(start[axis]+(end[axis]-start[axis])*step/20) for axis in (0,1)]
                        xdo('mousemove','--sync',str(point[0]),str(point[1]));time.sleep(.03)
                finally:xdo('mouseup','1')
                time.sleep(.5)
            try:
                for mode in ('source','preview'):
                    show_table(mode)
                    bounds=table_bounds()
                    assert bounds, 'The ordinary table must be visible for this comparison'
                    ImageGrab.grab(xdisplay=display).save(output / ('obsidian-table-'+('live' if mode=='source' else 'reading')+'.png'))
                    report['actions'].append({'view':mode,'tables':bounds,'viewing_preserved_bytes':note.read_text()==source})
                show_table('source')
                cell=table_bounds()[0]['columns'][0]
                xdo('mousemove','--sync',str(round(cell[0]+cell[2]/2)),str(round(cell[1]+cell[3]/2)),'click','3');time.sleep(.4)
                report['table_context_menu']=evaluate("JSON.stringify([...document.querySelectorAll('.menu-item-title')].map(e=>e.textContent))")
                ImageGrab.grab(xdisplay=display).save(output/'obsidian-table-menu.png')
                xdo('key','Escape')
                before=table_bounds();cell=before[0]['columns'][0]
                move([cell[0]+cell[2],cell[1]+cell[3]/2],[cell[0]+cell[2]+100,cell[1]+cell[3]/2])
                report['border_drag']={'before':before,'after':table_bounds(),'preserved_bytes':note.read_text()==source,'source_after':note.read_text()}
                for name in ('Alpha','Beta','Gamma','Delta'):
                    evaluate('(app.workspace.getLeaf("tab").setViewState({type:"markdown",state:{file:'+json.dumps('Drag '+name+'.md')+',mode:"source",source:false}}),JSON.stringify("opening"))')
                    time.sleep(.2)
                def tabs():
                    return evaluate("JSON.stringify([...document.querySelectorAll('.workspace-tabs.mod-active .workspace-tab-header')].map(e=>{const r=e.getBoundingClientRect();return {title:e.textContent.trim(),rect:[r.x,r.y,r.width,r.height]}}))")
                before=tabs()
                a=next(tab['rect'] for tab in before if tab['title']=='Drag Alpha')
                c=next(tab['rect'] for tab in before if tab['title']=='Drag Gamma')
                move([a[0]+20,a[1]+a[3]/2],[c[0]+c[2]-30,c[1]+c[3]/2])
                report['tab_drag']={'before':before,'after':tabs()}
                xdo('mousemove','--sync','1300','700');time.sleep(.6)
                ImageGrab.grab(xdisplay=display).save(output/'obsidian-tab-drag.png')
                report['fixture_exact']=note.read_text()==source
                report['fixture_after']=note.read_text()
            finally:
                (output/'observations.json').write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps({'section':'ux','status':'observed','output':str(output)}))
            raise SystemExit(0)

        if arguments.section == "empty-commands":
            report={**reference_identity,"profile":"disposable","status":"observed"}
            originals={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
            def palette_rows(label):
                xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2)
                xdo('type','--clearmodifiers',label);time.sleep(.3)
                return evaluate("JSON.stringify([...document.querySelectorAll('.suggestion-item')].map(e=>e.textContent.trim()))")
            try:
                xdo('key','--clearmodifiers','ctrl+e');time.sleep(.3)
                label='Toggle Live Preview/Source mode'
                report['document_rows']=palette_rows(label)
                assert any(label.lower() in row.lower() for row in report['document_rows']),report
                xdo('key','Return');time.sleep(.3)
                report['document_state']=evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())")
                xdo('key','--clearmodifiers','ctrl+t');time.sleep(.4)
                report['empty_state']=evaluate("JSON.stringify(app.workspace.activeLeaf.getViewState())")
                report['empty_rows']=palette_rows(label)
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-empty-mode-palette.png')
                xdo('key','Escape');time.sleep(.2)
                assert report['empty_state']['type']=='empty',report['empty_state']
                assert all(hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in originals.items())
                report.update(status='observed',original_notes_unchanged=True)
                print(json.dumps({'section':'empty-commands','document_rows':report['document_rows'],'empty_rows':report['empty_rows']}))
            finally:(ROOT/'reports/obsidian-empty-commands-reference.json').write_text(json.dumps(report,indent=2)+'\n')
            raise SystemExit(0)

        if arguments.section in ("native-highlights", "native-highlight-caret", "native-highlight-render", "native-highlight-availability", "native-highlight-empty"):
            report={**reference_identity,"profile":"disposable","status":"observed","cases":[]}
            note=vault/'Native highlights.md';original=note.read_text()
            originals={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md') if p!=note}
            def palette(label):
                xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2)
                xdo('type','--clearmodifiers',label);time.sleep(.3)
                selected=evaluate("JSON.stringify(document.querySelector('.suggestion-item.is-selected')?.textContent||'')")
                assert label.lower() in selected.lower(),(label,selected)
                xdo('key','Return');time.sleep(.3)
            def saved(expected=None):
                deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    value=note.read_text()
                    if (value!=original if expected is None else value==expected):return value
                    time.sleep(.05)
                raise AssertionError(('The reference did not save the expected edit',expected,note.read_text()))
            try:
                xdo('key','--clearmodifiers','ctrl+o');time.sleep(.2)
                xdo('type','--clearmodifiers','Native highlights');time.sleep(.3);xdo('key','Return');time.sleep(.6)
                xdo('key','--clearmodifiers','ctrl+e');time.sleep(.3)
                palette('Toggle Live Preview/Source mode')
                state=evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())")
                assert state.get('source') is True,state
                if arguments.section == "native-highlight-empty":
                    def observed():
                        state=evaluate("JSON.stringify((()=>{const e=app.workspace.activeLeaf.view.editor;return {text:e.getValue(),selection:e.getSelection(),cursor:e.getCursor()}})())")
                        saved(state['text']);return state
                    xdo('key','--clearmodifiers','ctrl+Home');xdo('key',*(['Right']*7))
                    before=observed();palette('Toggle blue highlight');empty=observed()
                    report['cases'].append({'name':'Apply Blue between spaces','before':before,'after':empty})
                    palette('Toggle blue highlight');repeated=observed()
                    report['cases'].append({'name':'Repeat Blue at an empty mark','after':repeated})
                    if repeated['text']!=empty['text']:xdo('key','--clearmodifiers','ctrl+z');saved(empty['text'])
                    xdo('key','--clearmodifiers','ctrl+e');time.sleep(.4)
                    reading=evaluate("JSON.stringify({text:document.querySelector('.workspace-leaf.mod-active .markdown-preview-view')?.innerText,marks:[...document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view mark')].map(e=>({text:e.textContent,color:e.dataset.highlight,html:e.outerHTML}))})")
                    report['cases'].append({'name':'Empty mark beside a real highlight in Reading','rendered':reading})
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-native-empty-highlight-reading.png')
                    xdo('key','--clearmodifiers','ctrl+e');time.sleep(.3)
                    xdo('type','--clearmodifiers','NEW');time.sleep(.4);typed=observed()
                    report['cases'].append({'name':'Type into the retained empty mark','after':typed})
                    xdo('key','--clearmodifiers','ctrl+z');saved(empty['text'])
                    xdo('key','--clearmodifiers','ctrl+z');saved(original)
                    assert all(hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in originals.items())
                    report.update(status='observed',original_notes_unchanged=True,fixture_restored=True)
                    print(json.dumps({'section':'native-highlight-empty','status':'observed','cases':len(report['cases'])}))
                    raise SystemExit(0)
                if arguments.section == "native-highlight-availability":
                    def palette_rows(label):
                        xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2);xdo('type','--clearmodifiers',label);time.sleep(.3)
                        return evaluate("JSON.stringify([...document.querySelectorAll('.suggestion-item')].map(e=>e.textContent.trim()))")
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','--clearmodifiers','ctrl+shift+Left')
                    rows=palette_rows('Toggle blue highlight');report['cases'].append({'mode':'Source','rows':rows});xdo('key','Escape');time.sleep(.2)
                    assert any('Toggle blue highlight' in row for row in rows),rows
                    xdo('key','--clearmodifiers','ctrl+e');time.sleep(.4)
                    assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())")['mode']=='preview'
                    rows=palette_rows('Toggle blue highlight');report['cases'].append({'mode':'Reading','rows':rows})
                    if any('Toggle blue highlight' in row for row in rows):
                        xdo('key','Return');time.sleep(.5)
                        report['reading_execution']={'state':evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())"),'text':note.read_text()}
                        if note.read_text()!=original:
                            if evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())")['mode']=='preview':xdo('key','--clearmodifiers','ctrl+e');time.sleep(.3)
                            xdo('key','--clearmodifiers','ctrl+z');saved(original)
                    else:xdo('key','Escape');time.sleep(.2)
                    xdo('key','--clearmodifiers','ctrl+t');time.sleep(.3)
                    rows=palette_rows('Toggle blue highlight');report['cases'].append({'mode':'Empty','rows':rows});xdo('key','Escape')
                    assert note.read_text()==original and all(hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in originals.items())
                    report.update(status='observed',original_notes_unchanged=True,fixture_restored=True)
                    print(json.dumps({'section':'native-highlight-availability','status':'observed','cases':report['cases']}))
                    raise SystemExit(0)
                if arguments.section == "native-highlight-render":
                    xdo('key','--clearmodifiers','ctrl+o');time.sleep(.2);xdo('type','--clearmodifiers','Native highlight rendering');time.sleep(.3);xdo('key','Return');time.sleep(.6)
                    rendered_note=vault/'Native highlight rendering.md';rendered_source=rendered_note.read_text()
                    for appearance in ('dark','light'):
                        if not evaluate("JSON.stringify(document.body.classList.contains('theme-"+appearance+"'))"):
                            palette('Toggle light/dark mode')
                        assert evaluate("JSON.stringify(document.body.classList.contains('theme-"+appearance+"'))")
                        if evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())")['mode']!='preview':
                            xdo('key','--clearmodifiers','ctrl+e');time.sleep(.4)
                        marks=evaluate("JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view mark')].map(e=>{const s=getComputedStyle(e);return {text:e.textContent,highlight:e.dataset.highlight||null,background:s.backgroundColor,color:s.color,html:e.outerHTML}}))")
                        literal=evaluate("JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view code')].map(e=>({text:e.textContent,marks:e.querySelectorAll('mark').length})))")
                        assert all(any(m['text']=='TARGET_'+c.upper() and m['highlight']==c.lower() for m in marks) for c in ('Blue','Green','Orange','Purple','Red','Yellow')),marks
                        assert all(c['marks']==0 for c in literal) and any('==🔵literal==' in c['text'] for c in literal) and any('==🟣fenced==' in c['text'] for c in literal),literal
                        ImageGrab.grab(xdisplay=display).save(ROOT/f'reports/obsidian-native-highlight-render-{appearance}-reading.png')
                        report['cases'].append({'appearance':appearance,'mode':'Reading','marks':marks,'literal_code':literal})
                        xdo('key','--clearmodifiers','ctrl+e');time.sleep(.4)
                        if evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())").get('source',False):palette('Toggle Live Preview/Source mode')
                        assert not evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())").get('source',False)
                        live=evaluate("JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .cm-content span')].filter(e=>e.textContent.startsWith('TARGET_')).map(e=>{const s=getComputedStyle(e);return {text:e.textContent,classes:e.className,background:s.backgroundColor,color:s.color,html:e.outerHTML}}))")
                        assert live,('Actual Live rendered highlight spans must appear',appearance)
                        ImageGrab.grab(xdisplay=display).save(ROOT/f'reports/obsidian-native-highlight-render-{appearance}-live.png')
                        report['cases'].append({'appearance':appearance,'mode':'Live preview','spans':live})
                        assert rendered_note.read_text()==rendered_source
                    assert all(hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in originals.items())
                    report.update(status='passed',original_notes_unchanged=True,fixture_restored=note.read_text()==original)
                    print(json.dumps({'section':'native-highlight-render','status':'passed','cases':len(report['cases'])}))
                    raise SystemExit(0)
                if arguments.section == "native-highlight-caret":
                    def observed():
                        state=evaluate("JSON.stringify((()=>{const e=app.workspace.activeLeaf.view.editor;return {text:e.getValue(),selection:e.getSelection(),cursor:e.getCursor()}})())")
                        saved(state['text']);return state
                    def observe_toggle(name,label):
                        before=observed();palette(label);after=observed()
                        report['cases'].append({'name':name,'before':before,'after':after})
                        if after['text']!=before['text']:
                            xdo('key','--clearmodifiers','ctrl+z');saved(before['text'])
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','Left','Left','Left')
                    observe_toggle('Apply Blue at an unmarked interior caret','Toggle blue highlight')
                    xdo('key','--clearmodifiers','ctrl+End')
                    observe_toggle('Apply Blue at an empty selection at the document end','Toggle blue highlight')
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','--clearmodifiers','ctrl+shift+Left')
                    palette('Toggle blue highlight');blue=observed()
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','Left','Left','Left');xdo('key','--clearmodifiers','shift+Left','shift+Left','shift+Left')
                    observe_toggle('Apply Purple to a partial selection inside Blue','Toggle purple highlight')
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','Left','Left','Left')
                    observe_toggle('Apply Purple at a caret inside Blue','Toggle purple highlight')
                    xdo('key','--clearmodifiers','ctrl+z');saved(original)
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','--clearmodifiers','ctrl+shift+Left')
                    palette('Toggle highlight');plain=observed()
                    observe_toggle('Apply Yellow to the generic uncolored highlight','Toggle yellow highlight')
                    xdo('key','--clearmodifiers','ctrl+z');saved(original)
                    assert all(hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in originals.items())
                    report.update(status='observed',original_notes_unchanged=True,fixture_restored=note.read_text()==original)
                    print(json.dumps({'section':'native-highlight-caret','status':'observed','cases':len(report['cases'])}))
                    raise SystemExit(0)
                for color in ('Blue','Green','Orange','Purple','Red','Yellow'):
                    xdo('key','--clearmodifiers','ctrl+End');xdo('key','--clearmodifiers','ctrl+shift+Left')
                    assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.editor.getSelection())")=='TARGET'
                    palette('Toggle '+color.lower()+' highlight')
                    changed=saved();assert changed.count('TARGET')==1 and '==literal==' in changed
                    xdo('key','--clearmodifiers','ctrl+e');time.sleep(.4)
                    rendered=evaluate("JSON.stringify([...document.querySelectorAll('.markdown-preview-view *')].filter(e=>e.textContent==='TARGET'&&e.getBoundingClientRect().height).map(e=>{const s=getComputedStyle(e),r=e.getBoundingClientRect();return {tag:e.tagName,html:e.outerHTML,color:s.color,background:s.backgroundColor,rect:[r.x,r.y,r.width,r.height]}}))")
                    assert rendered,('The highlighted target must appear in Reading',color)
                    literal=evaluate("JSON.stringify([...document.querySelectorAll('.markdown-preview-view code')].map(e=>({text:e.textContent,marks:e.querySelectorAll('mark').length})))")
                    assert any(c['text']=='==literal==' and c['marks']==0 for c in literal),literal
                    ImageGrab.grab(xdisplay=display).save(ROOT/f'reports/obsidian-native-highlight-{color.lower()}.png')
                    xdo('key','--clearmodifiers','ctrl+e');time.sleep(.3)
                    xdo('key','--clearmodifiers','ctrl+z');saved(original)
                    xdo('key','--clearmodifiers','ctrl+shift+z');saved(changed)
                    xdo('key','--clearmodifiers','ctrl+z');saved(original)
                    report['cases'].append({'color':color,'source':changed,'reading':rendered,'literal_code_unchanged':True,'single_undo_redo_exact':True})
                assert note.read_text()==original
                report['toggle_cases']=[]
                def edit_state():
                    state=evaluate("JSON.stringify((()=>{const e=app.workspace.activeLeaf.view.editor;return {text:e.getValue(),selection:e.getSelection(),cursor:e.getCursor()}})())")
                    saved(state['text'])
                    return state
                xdo('key','--clearmodifiers','ctrl+End');xdo('key','--clearmodifiers','ctrl+shift+Left')
                palette('Toggle blue highlight');blue=edit_state()
                report['toggle_cases'].append({'name':'Apply Blue','state':blue})
                palette('Toggle blue highlight');toggled=edit_state()
                report['toggle_cases'].append({'name':'Repeat same color on the retained selection','state':toggled})
                xdo('key','--clearmodifiers','ctrl+z');saved(blue['text'])
                palette('Toggle purple highlight');purple=edit_state()
                report['toggle_cases'].append({'name':'Change Blue to Purple on the retained selection','state':purple})
                xdo('key','--clearmodifiers','ctrl+z');saved(blue['text'])
                xdo('key','--clearmodifiers','ctrl+End');xdo('key','Left','Left','Left')
                before_caret=edit_state()
                assert before_caret['selection']=='',before_caret
                palette('Toggle blue highlight');caret=edit_state()
                report['toggle_cases'].append({'name':'Toggle Blue at an interior caret without a selection','before':before_caret,'state':caret})
                xdo('key','--clearmodifiers','ctrl+z');saved(blue['text'])
                xdo('key','--clearmodifiers','ctrl+z');saved(original)
                assert all(hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in originals.items())
                report.update(status='passed',original_notes_unchanged=True,fixture_restored=True)
                print(json.dumps({'section':'native-highlights','status':'passed','colors':[row['color'] for row in report['cases']],'toggle_observations':len(report['toggle_cases'])}))
            finally:
                (ROOT/f"reports/obsidian-{arguments.section}-reference.json").write_text(json.dumps(report,indent=2)+'\n')
            raise SystemExit(0)

        if arguments.section in ("page-preview", "page-preview-actions"):
            report={**reference_identity,'profile':'disposable','status':'observed','cases':[]}
            originals={p.relative_to(vault).as_posix():p.read_bytes() for p in vault.rglob('*.md')}
            def popovers():
                return evaluate("JSON.stringify({active:app.workspace.activeLeaf?.view.file?.path||null,popovers:[...document.querySelectorAll('.hover-popover')].filter(e=>e.getBoundingClientRect().width&&e.getBoundingClientRect().height).map(e=>({text:e.innerText,rect:(()=>{const r=e.getBoundingClientRect();return [r.x,r.y,r.width,r.height]})()}))})")
            try:
                xdo('key','ctrl+o');time.sleep(.2);xdo('type','--clearmodifiers','Preview source');time.sleep(.3);xdo('key','Return');time.sleep(.7)
                if arguments.section == "page-preview-actions":
                    report['actions']=[]
                    def at(selector):
                        rect=evaluate("JSON.stringify((()=>{const e=document.querySelector("+json.dumps(selector)+");if(!e)return null;const r=e.getBoundingClientRect();return r.width&&r.height?[screenX+r.x+r.width/2,screenY+r.y+r.height/2]:null})())")
                        assert rect,('Actual preview control must be visible',selector)
                        xdo('mousemove',*[str(round(v)) for v in rect]);return rect
                    def hover(href='Preview target'):
                        xdo('mousemove','30','930');time.sleep(.6)
                        at('.workspace-leaf.mod-active a.internal-link[data-href="'+href+'"]');time.sleep(.9)
                        return popovers()
                    shown=hover();assert len(shown['popovers'])==1 and shown['active']=='Preview source.md',shown
                    report['controls']=evaluate("JSON.stringify([...document.querySelectorAll('.hover-popover button,.hover-popover input,.hover-popover .clickable-icon')].map(e=>({tag:e.tagName,type:e.type||null,label:e.getAttribute('aria-label'),title:e.getAttribute('title'),checked:e.checked,disabled:e.disabled,html:e.outerHTML})))")
                    card=evaluate("JSON.stringify((()=>{const r=document.querySelector('.hover-popover').getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                    xdo('mousemove',*[str(round(v)) for v in card]);time.sleep(.7)
                    stayed=popovers();assert stayed['popovers'] and stayed['active']=='Preview source.md',stayed
                    report['actions'].append({'name':'Pointer entered and stayed within the card','state':stayed})
                    target_note=vault/'Preview target.md';target_original=target_note.read_text()
                    at('.hover-popover input[type="checkbox"]');xdo('click','1');time.sleep(.7)
                    after=target_note.read_text()
                    report['actions'].append({'name':'Click task checkbox','before':target_original,'after':after,'state':popovers()})
                    if after!=target_original:
                        at('.hover-popover input[type="checkbox"]');xdo('click','1');time.sleep(.7)
                        assert target_note.read_text()==target_original
                    report['focus_after_task']=evaluate("JSON.stringify((()=>{const e=document.activeElement;return {tag:e?.tagName,type:e?.type||null,in_card:!!e?.closest('.hover-popover')}})())")
                    xdo('mousemove','30','930');time.sleep(.7)
                    report['pointer_away_after_task']=popovers()
                    def dismiss_card():
                        xdo('mousemove','30','930');time.sleep(.7)
                        if popovers()['popovers']:
                            xdo('key','Escape');time.sleep(.3)
                            report.setdefault('dismissals',[]).append({'gesture':'Escape','state':popovers()})
                        if popovers()['popovers']:
                            xdo('mousemove','1100','800','click','1');time.sleep(.5)
                            report.setdefault('dismissals',[]).append({'gesture':'Outside click','state':popovers()})
                        assert popovers()['popovers']==[]
                    dismiss_card();hover()
                    report['card_html']=evaluate("JSON.stringify(document.querySelector('.hover-popover')?.outerHTML)")
                    icons=evaluate("JSON.stringify([...document.querySelectorAll('.hover-popover .clickable-icon,.hover-popover .markdown-embed-link,.hover-popover [role=button]')].map(e=>({label:e.getAttribute('aria-label'),title:e.getAttribute('title'),classes:e.className,html:e.outerHTML})))")
                    report['card_icons']=icons
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-page-preview-actions.png')
                    embed=evaluate("JSON.stringify(!!document.querySelector('.hover-popover .markdown-embed-link'))")
                    if embed:
                        before=evaluate("JSON.stringify({active:app.workspace.activeLeaf?.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').map(l=>l.view.file?.path||null)})")
                        at('.hover-popover .markdown-embed-link');xdo('click','1');time.sleep(.6)
                        after=evaluate("JSON.stringify({active:app.workspace.activeLeaf?.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').map(l=>l.view.file?.path||null)})")
                        report['actions'].append({'name':'Open link from the corner control','before':before,'after':after})
                        xdo('key','--clearmodifiers','ctrl+o');time.sleep(.2);xdo('type','--clearmodifiers','Preview source');time.sleep(.3);xdo('key','Return');time.sleep(.5)
                    dismiss_card();hover()
                    at('.hover-popover a.internal-link');xdo('click','1');time.sleep(.6)
                    report['actions'].append({'name':'Follow the card internal link','after':evaluate("JSON.stringify({active:app.workspace.activeLeaf?.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').map(l=>l.view.file?.path||null)})")})
                    xdo('key','--clearmodifiers','ctrl+o');time.sleep(.2);xdo('type','--clearmodifiers','Preview source');time.sleep(.3);xdo('key','Return');time.sleep(.5)
                    dismiss_card()
                    report['missing_hover']=hover('Missing preview target')
                    assert report['missing_hover']['active']=='Preview source.md' and not (vault/'Missing preview target.md').exists()
                    dismiss_card()
                    assert all((vault/name).read_bytes()==data for name,data in originals.items())
                    report.update(status='observed',notes_unchanged=True)
                    print(json.dumps({'section':'page-preview-actions','status':'observed','actions':len(report['actions']),'icons':icons}))
                    raise SystemExit(0)
                for mode in ('Reading','Live preview','Source'):
                    xdo('mousemove','30','930');time.sleep(.6)
                    if mode=='Live preview':
                        xdo('key','ctrl+e');time.sleep(.5)
                    elif mode=='Source':
                        xdo('key','ctrl+p');time.sleep(.2);xdo('type','--clearmodifiers','Toggle Live Preview/Source mode');time.sleep(.3);xdo('key','Return');time.sleep(.5)
                    view=evaluate("JSON.stringify(app.workspace.activeLeaf.view.getState())")
                    report.setdefault('views',[]).append({'mode':mode,'state':view})
                    assert view['mode']==('preview' if mode=='Reading' else 'source'),view
                    if mode!='Reading':assert view.get('source',False)==(mode=='Source'),view
                    for control in (False,True):
                        xdo('mousemove','30','930');xdo('keyup','Control_L');time.sleep(.6)
                        assert popovers()['popovers']==[]
                        selector='a.internal-link[data-href="Preview target"]' if mode=='Reading' else '.cm-hmd-internal-link'
                        rect=evaluate("JSON.stringify((()=>{const e=document.querySelector('.workspace-leaf.mod-active '+"+json.dumps(selector)+");if(!e)return null;const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                        assert rect,('The actual preview link must be present',mode)
                        if control:xdo('keydown','Control_L')
                        xdo('mousemove',*[str(round(v)) for v in rect]);time.sleep(.9)
                        state=popovers();state.update(control=control,mode=mode)
                        report['cases'].append(state)
                        ImageGrab.grab(xdisplay=display).save(ROOT/f'reports/obsidian-page-preview-{mode.lower().replace(" ","-")}-{int(control)}.png')
                        assert state['active']=='Preview source.md'
                        xdo('keyup','Control_L')
                assert any('VISIBLE_PREVIEW_DESTINATION' in p['text'] for state in report['cases'] for p in state['popovers'])
                xdo('mousemove','30','930');time.sleep(.7)
                assert popovers()['popovers']==[]
                assert all((vault/name).read_bytes()==data for name,data in originals.items())
                report['notes_unchanged']=True;report['status']='passed'
                print(json.dumps({'section':'page-preview','status':'passed','cases':[(c['mode'],c['control'],len(c['popovers'])) for c in report['cases']]}))
            finally:
                xdo('keyup','Control_L')
                (ROOT/f"reports/obsidian-{arguments.section}-reference.json").write_text(json.dumps(report,indent=2)+'\n')
            raise SystemExit(0)

        if arguments.section in ("file-actions", "file-move"):
            report={**reference_identity,"profile":"disposable","status":"observed","actions":[]}
            original_notes={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
            sample=vault/'Notes/Menu sample.md';sample.write_bytes(b'# Menu sample\r\n\r\nKeep exact bytes.\r\n')
            collision=vault/'Notes/Menu sample 1.md';collision.write_bytes(b'KEEP_COLLISION\n')
            source=sample.read_bytes();time.sleep(.6)
            def click_element(expression,button='1'):
                rect=evaluate("JSON.stringify((()=>{const e="+expression+";if(!e)return null;e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect();return r.width&&r.height?[screenX+r.x+r.width/2,screenY+r.y+r.height/2]:null})())")
                assert rect,('Missing file-menu element',expression)
                xdo('mousemove',*[str(round(v)) for v in rect],'click',button);time.sleep(.3)
            def menu(path):
                parts=Path(path).parts[:-1]
                for count in range(1,len(parts)+1):
                    prefix='/'.join(parts[:count])
                    folder="[...document.querySelectorAll('.nav-folder-title')].find(e=>e.dataset.path==="+json.dumps(prefix)+")"
                    if evaluate("JSON.stringify(Boolean(("+folder+")?.closest('.nav-folder')?.classList.contains('is-collapsed')))"):
                        click_element(folder)
                click_element("[...document.querySelectorAll('.nav-file-title')].find(e=>e.dataset.path==="+json.dumps(path)+")",'3')
            def action(name):
                click_element("[...document.querySelectorAll('.menu-item')].find(e=>e.querySelector('.menu-item-title')?.textContent.trim()==="+json.dumps(name)+")")
            try:
                if arguments.section == "file-move":
                    sample.write_bytes(b'# Menu sample\n\nKeep exact bytes.\n')
                    source=sample.read_bytes()
                    folder=vault/'Move destination';folder.mkdir()
                    reference=vault/'Move reference.md'
                    reference.write_text('[[Notes/Menu sample|Sample alias]]\n\n[Sample](Notes/Menu%20sample.md)\n')
                    time.sleep(.7)
                    deadline=time.monotonic()+6
                    while time.monotonic()<deadline:
                        cache=evaluate("JSON.stringify(app.metadataCache.getCache('Move reference.md')||null)")
                        if cache and len(cache.get('links',[]))==2:break
                        time.sleep(.15)
                    assert cache and len(cache.get('links',[]))==2,cache
                    report['indexed_reference']=cache
                    report['automatic_links']=evaluate("JSON.stringify(app.vault.getConfig('alwaysUpdateLinks')??null)")
                    def move(path,query):
                        menu(path);action('Move file to...')
                        xdo('type','--clearmodifiers',query);time.sleep(.35)
                        return evaluate("JSON.stringify({body:document.body.innerText,rows:[...document.querySelectorAll('.suggestion-item')].map(e=>e.textContent.trim()),inputs:[...document.querySelectorAll('input')].filter(e=>e.getBoundingClientRect().height).map(e=>({value:e.value,placeholder:e.placeholder}))})")
                    def linked_to(path):
                        deadline=time.monotonic()+5
                        while time.monotonic()<deadline:
                            state=evaluate("JSON.stringify({targets:(app.metadataCache.getCache('Move reference.md')?.links||[]).map(l=>app.metadataCache.getFirstLinkpathDest(decodeURIComponent(l.link),'Move reference.md')?.path||null)})")
                            state['text']=reference.read_text()
                            if state['targets']==[path,path]:return state
                            time.sleep(.15)
                        raise AssertionError(('Moved links must resolve to the actual destination',path,state))
                    same_before={p.relative_to(vault).as_posix():p.read_bytes() for p in vault.rglob('*.md')}
                    report['same_folder_picker']=move('Notes/Menu sample.md','Notes')
                    assert 'Notes' in report['same_folder_picker']['rows'],report['same_folder_picker']
                    xdo('key','Return');time.sleep(.6)
                    report['same_folder_ui']=evaluate("JSON.stringify({modal:!!document.querySelector('.modal-container'),active:app.workspace.activeLeaf?.view.file?.path,notices:[...document.querySelectorAll('.notice')].map(e=>e.textContent)})")
                    same_after={p.relative_to(vault).as_posix():p.read_bytes() for p in vault.rglob('*.md')}
                    report['same_folder_unchanged']=same_after==same_before
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-same-folder.png')
                    assert same_after==same_before,('Same-folder move changed files',sorted(same_after.keys()-same_before.keys()),report['same_folder_ui'])
                    assert not report['same_folder_ui']['modal'],report['same_folder_ui']
                    report['actions'].append({'name':'Choose current folder','files_unchanged':True,'picker_closed':True})
                    report['fuzzy_folder_ui']=move('Notes/Menu sample.md','Move dest')
                    report['fuzzy_folder_selected']=evaluate("JSON.stringify(document.querySelector('.suggestion-item.is-selected')?.textContent||null)")
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-fuzzy.png')
                    xdo('key','Escape');time.sleep(.2)
                    assert sample.read_bytes()==source and not (folder/sample.name).exists()
                    report['actions'].append({'name':'Fuzzy folder suggestions and cancellation','rows':report['fuzzy_folder_ui']['rows'],'selected':report['fuzzy_folder_selected']})
                    report['cancel_ui']=move('Notes/Menu sample.md','Move destination')
                    assert report['cancel_ui']['rows']==['Move destination']
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-filtered.png')
                    xdo('key','Escape');time.sleep(.2)
                    assert sample.read_bytes()==source and not (folder/sample.name).exists()
                    report['actions'].append({'name':'Cancel filtered move','source_unchanged':True})
                    report['move_ui']=move('Notes/Menu sample.md','Move destination')
                    xdo('key','Return');time.sleep(.8)
                    moved=folder/sample.name
                    assert not sample.exists() and moved.read_bytes()==source
                    report['linked_after_move']=reference.read_text()
                    report['after_move_ui']=evaluate("JSON.stringify({body:document.body.innerText,buttons:[...document.querySelectorAll('.modal-container button')].map(e=>e.textContent.trim())})")
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-after.png')
                    assert report['after_move_ui']['buttons']==['Always update','Just once','Do not update']
                    click_element("[...document.querySelectorAll('.modal-container button')].find(e=>e.textContent.trim()==='Just once')")
                    time.sleep(.4)
                    assert evaluate("JSON.stringify(app.vault.getConfig('alwaysUpdateLinks'))") is False
                    report['resolved_after_once']=linked_to(moved.relative_to(vault).as_posix())
                    assert '|Sample alias]]' in reference.read_text() and '[Sample](' in reference.read_text()
                    report['actions'].append({'name':'Move to existing folder','path':moved.relative_to(vault).as_posix(),'active':evaluate("JSON.stringify(app.workspace.activeLeaf?.view.file?.path||null)"),'reference':reference.read_text()})
                    report['plain_enter_new_folder_ui']=move(moved.relative_to(vault).as_posix(),'Enter parent/Nested')
                    xdo('key','Return');time.sleep(.8)
                    enter_created=vault/'Enter parent/Nested'/sample.name
                    report['plain_enter_created']=enter_created.is_file()
                    report['plain_enter_modal']=evaluate("JSON.stringify({buttons:[...document.querySelectorAll('.modal-container button')].map(e=>e.textContent.trim()),inputs:[...document.querySelectorAll('.modal-container input')].map(e=>e.value)})")
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-plain-enter.png')
                    assert enter_created.is_file() and enter_created.read_bytes()==source and not moved.exists(),report['plain_enter_modal']
                    assert report['plain_enter_modal']['buttons']==[]
                    linked_to(enter_created.relative_to(vault).as_posix())
                    move(enter_created.relative_to(vault).as_posix(),'Move destination');xdo('key','Return');time.sleep(.6)
                    assert moved.read_bytes()==source and not enter_created.exists()
                    linked_to(moved.relative_to(vault).as_posix())
                    report['actions'].append({'name':'Plain Enter creates the offered new folder','exact_bytes':True,'restored_existing_folder':True})
                    report['new_folder_ui']=move(moved.relative_to(vault).as_posix(),'New parent/Nested')
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-create.png')
                    xdo('key','shift+Return');time.sleep(.8)
                    nested=vault/'New parent/Nested'/sample.name
                    assert nested.is_file() and nested.read_bytes()==source and not moved.exists()
                    report['nested_prompt']=evaluate("JSON.stringify({buttons:[...document.querySelectorAll('.modal-container button')].map(e=>e.textContent.trim())})")
                    assert report['nested_prompt']['buttons']==[]
                    report['resolved_after_nested']=linked_to(nested.relative_to(vault).as_posix())
                    assert reference.read_text()==report['resolved_after_once']['text']
                    report['actions'].append({'name':'Create nested folder and move','path':nested.relative_to(vault).as_posix(),'reference':reference.read_text(),'no_rewrite_prompt_needed':True})
                    # Use explicit paths again to exercise the persistent choice.
                    reference.write_text('[[New parent/Nested/Menu sample|Sample alias]]\n\n[Sample](New%20parent/Nested/Menu%20sample.md)\n')
                    deadline=time.monotonic()+5
                    while time.monotonic()<deadline:
                        links=evaluate("JSON.stringify((app.metadataCache.getCache('Move reference.md')?.links||[]).map(l=>l.link))")
                        if links and all(l.startswith(('New parent/','New%20parent/')) for l in links):break
                        time.sleep(.15)
                    assert len(links)==2 and all(l.startswith(('New parent/','New%20parent/')) for l in links),links
                    report['explicit_root_fixture']=reference.read_text()
                    report['root_ui']=move(nested.relative_to(vault).as_posix(),'/')
                    xdo('key','Return');time.sleep(.8)
                    root_file=vault/sample.name
                    assert root_file.is_file() and root_file.read_bytes()==source and not nested.exists()
                    report['root_prompt']=evaluate("JSON.stringify({buttons:[...document.querySelectorAll('.modal-container button')].map(e=>e.textContent.trim())})")
                    assert report['root_prompt']['buttons']==['Always update','Just once','Do not update']
                    click_element("[...document.querySelectorAll('.modal-container button')].find(e=>e.textContent.trim()==='Always update')")
                    time.sleep(.4)
                    assert evaluate("JSON.stringify(app.vault.getConfig('alwaysUpdateLinks'))") is True
                    report['resolved_after_root']=linked_to(root_file.name)
                    assert not evaluate("JSON.stringify(Boolean(document.querySelector('.modal-container')))")
                    report['actions'].append({'name':'Move to vault root','path':root_file.name,'reference':reference.read_text()})
                    (folder/root_file.name).write_bytes(b'KEEP_MOVE_COLLISION\n');time.sleep(.4)
                    before_collision={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}
                    report['collision_picker']=move(root_file.name,'Move destination')
                    xdo('key','Return');time.sleep(.5)
                    report['collision_ui']=evaluate("JSON.stringify({body:document.body.innerText,modal:Boolean(document.querySelector('.modal-container')),active:app.workspace.activeLeaf?.view.file?.path||null})")
                    after_collision={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}
                    added=after_collision-before_collision
                    assert added=={'Move destination/Menu sample 1.md'},added
                    numbered=vault/next(iter(added))
                    assert not root_file.exists() and numbered.read_bytes()==source
                    assert (folder/root_file.name).read_bytes()==b'KEEP_MOVE_COLLISION\n'
                    report['resolved_after_collision']=linked_to(numbered.relative_to(vault).as_posix())
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-move-collision.png')
                    xdo('key','Escape');time.sleep(.2)
                    report['actions'].append({'name':'Occupied destination chooses numbered name','path':numbered.relative_to(vault).as_posix(),'exact_bytes':True,'occupied_target_unchanged':True,'no_further_prompt':not report['collision_ui']['modal']})
                    assert all((vault/name).is_file() and hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in original_notes.items())
                    report['status']='passed';report['notes_unchanged']=True
                    print(json.dumps({'section':'file-move','status':'passed','actions':len(report['actions'])}))
                    raise SystemExit(0)
                before={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}
                menu('Notes/Menu sample.md');action('Make a copy')
                time.sleep(.7)
                report['copy_ui']=evaluate("JSON.stringify({body:document.body.innerText,active:app.workspace.activeLeaf?.view.file?.path,inputs:[...document.querySelectorAll('input')].filter(e=>e.getBoundingClientRect().height).map(e=>({value:e.value,placeholder:e.placeholder}))})")
                created={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}-before
                report['created']=sorted(created)
                assert len(created)==1,('Make a copy did not immediately create one file',report['copy_ui'])
                copied=next(iter(created));assert (vault/copied).read_bytes()==source
                assert collision.read_bytes()==b'KEEP_COLLISION\n' and sample.read_bytes()==source
                report['actions'].append({'name':'Make a copy','created':copied,'active':report['copy_ui']['active'],'exact_bytes':True,'collision_unchanged':True})
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-file-copy.png')
                before_second={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}
                menu(copied);action('Make a copy');time.sleep(.5)
                more={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}-before_second
                assert len(more)==1
                copy_of_copy=next(iter(more));assert (vault/copy_of_copy).read_bytes()==source
                report['actions'].append({'name':'Copy of a numbered copy','created':copy_of_copy,'exact_bytes':True})
                before_move=(vault/copied).read_bytes()
                report['first_copy_after_second']=before_move.decode()
                report['second_copy_after_creation']=(vault/copy_of_copy).read_bytes().decode()
                menu(copied);action('Move file to...')
                report['move_ui']=evaluate("JSON.stringify({body:document.body.innerText,inputs:[...document.querySelectorAll('input')].filter(e=>e.getBoundingClientRect().height).map(e=>({value:e.value,placeholder:e.placeholder})),rows:[...document.querySelectorAll('.suggestion-item')].map(e=>e.textContent.trim())})")
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-file-move-picker.png')
                xdo('key','Escape');time.sleep(.25)
                assert (vault/copied).read_bytes()==before_move
                assert sample.read_bytes()==source and collision.read_bytes()==b'KEEP_COLLISION\n'
                report['actions'].append({'name':'Move cancellation','exact_bytes':True})
                image=vault/'Menu image.png';Image.new('RGB',(24,16),(47,157,212)).save(image)
                image_bytes=image.read_bytes();time.sleep(.6)
                menu('Menu image.png');action('Make a copy');time.sleep(.5)
                image_copy=vault/'Menu image 1.png'
                assert image_copy.is_file() and image_copy.read_bytes()==image_bytes and image.read_bytes()==image_bytes
                report['actions'].append({'name':'Binary image copy','created':image_copy.name,'exact_bytes':True,'active':evaluate("JSON.stringify(app.workspace.activeLeaf?.view.file?.path||null)")})
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-file-copy-image.png')
                report['notes_unchanged']=all((vault/name).is_file() and hashlib.sha256((vault/name).read_bytes()).hexdigest()==digest for name,digest in original_notes.items())
                assert report['notes_unchanged'];report['status']='passed'
                print(json.dumps({'section':'file-actions','status':'passed','created':copied,'move':report['move_ui']}))
            finally:
                (ROOT/f'reports/obsidian-{arguments.section}-reference.json').write_text(json.dumps(report,indent=2)+'\n')
            raise SystemExit(0)

        if arguments.section in ("tab-actions", "tab-windows"):
            report = {"appimage":str(arguments.appimage), "version":evaluate("JSON.stringify(app.appVersion||app.version||null)"), "profile":"disposable", "status":"observed", "actions":[]}
            original_notes={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
            def tabs_state():
                return evaluate("JSON.stringify({main:app.workspace.getLayout().main,active:app.workspace.activeLeaf?.view.file?.path,leaves:app.workspace.getLeavesOfType('markdown').map(l=>({file:l.view.file?.path,pinned:l.pinned})),groups:[...document.querySelectorAll('.mod-root .workspace-tabs')].map(e=>({stacked:e.classList.contains('mod-stacked'),titles:[...e.querySelectorAll('.workspace-tab-header-inner-title')].map(t=>t.textContent)}))})")
            def run_tab_command(name):
                xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2)
                xdo('type','--clearmodifiers','--delay','3',name);time.sleep(.25)
                choices=evaluate("JSON.stringify([...document.querySelectorAll('.suggestion-item .suggestion-content')].map(e=>e.textContent.trim()))")
                assert name in choices, ('Missing tab command',name,choices)
                xdo('key','Return');time.sleep(.5)
            def pick_note(name):
                xdo('key','--clearmodifiers','ctrl+o');time.sleep(.2)
                xdo('type','--clearmodifiers','--delay','3',name);time.sleep(.25)
                xdo('key','Return');time.sleep(.4)
            def tab_click(expression,button='1'):
                rect=evaluate("JSON.stringify((()=>{const e="+expression+";if(!e)return null;const r=e.getBoundingClientRect();return r.width&&r.height?[screenX+r.x+r.width/2,screenY+r.y+r.height/2]:null})())")
                assert rect, ('Missing tab control',expression)
                xdo('mousemove',*[str(round(value)) for value in rect],'click',button);time.sleep(.3)
            def keep(name):
                state=tabs_state();report['actions'].append({'name':name,'state':state});return state
            try:
                initial=keep('initial');assert initial['active']=='Home.md'
                if arguments.section == "tab-windows":
                    def windows():
                        return set(xdo('search','--onlyvisible','--class','obsidian').splitlines())
                    def wait_for(predicate, message):
                        deadline=time.monotonic()+6
                        while time.monotonic()<deadline:
                            result=predicate()
                            if result:return result
                            time.sleep(.1)
                        raise AssertionError(message)
                    def opened_window(before):
                        added=wait_for(lambda:windows()-before,'The reference did not create another visible window')
                        assert len(added)==1,added
                        return next(iter(added))
                    def close_window(popup):
                        xdo('windowfocus',popup);xdo('key','--clearmodifiers','ctrl+shift+w')
                        wait_for(lambda:popup not in windows(),'The reference popout did not close')
                        xdo('windowfocus',window);time.sleep(.3)
                    original_windows=windows()
                    run_tab_command('Open current tab in new window')
                    popup=opened_window(original_windows)
                    assert 'Home' in xdo('getwindowname',popup)
                    copied=keep('copy tab into new window')
                    assert len(copied['leaves'])==len(initial['leaves'])+1
                    assert copied['main']==initial['main']
                    report['copy_window_title']=xdo('getwindowname',popup)
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-tab-copy-window.png')
                    close_window(popup)
                    after_close=keep('close copied window');assert len(after_close['leaves'])==len(initial['leaves'])
                    run_tab_command('Move current tab to new window')
                    popup=opened_window(original_windows)
                    assert 'Home' in xdo('getwindowname',popup)
                    moved=keep('move tab into new window')
                    assert len(moved['leaves'])==len(initial['leaves'])
                    assert 'Home.md' not in json.dumps(moved['main'])
                    report['move_window_title']=xdo('getwindowname',popup)
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-tab-move-window.png')
                    close_window(popup)
                    after_move=keep('close moved window');assert not after_move['leaves']
                    pick_note('Home')
                    run_tab_command('New window')
                    popup=opened_window(original_windows)
                    fresh=keep('new empty window');assert len(fresh['leaves'])==len(initial['leaves'])
                    report['empty_window_title']=xdo('getwindowname',popup)
                    close_window(popup)
                    report['notes_unchanged']=original_notes=={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
                    assert report['notes_unchanged'];report['status']='passed'
                    print(json.dumps({'section':'tab-windows','status':'passed','actions':len(report['actions'])}))
                    raise SystemExit(0)
                run_tab_command('Toggle pin')
                pinned=keep('pin');assert sum(bool(l['pinned']) for l in pinned['leaves'])==1
                pick_note('Template target')
                opened=keep('navigate from pinned');assert opened['active']=='Template target.md'
                assert {'Home.md','Template target.md'}<={l['file'] for l in opened['leaves']}
                tab_click("document.querySelector('.mod-root .workspace-tab-header.is-active')",'3')
                report['menu']=evaluate("JSON.stringify([...document.querySelectorAll('.menu-item')].filter(e=>e.getBoundingClientRect().height).map(e=>({text:e.textContent.trim(),font:getComputedStyle(e).fontSize,height:e.getBoundingClientRect().height,disabled:e.classList.contains('is-disabled')})))")
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-tab-menu.png');xdo('key','Escape')
                run_tab_command('Split right')
                right=keep('split right');assert len(right['groups'])==len(opened['groups'])+1
                run_tab_command('Split down')
                down=keep('split down');assert len(down['groups'])==len(right['groups'])+1
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-tabs-split.png')
                run_tab_command('Toggle stacked tabs')
                stacked=keep('stacked');assert any(group['stacked'] for group in stacked['groups'])
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-tabs-stacked.png')
                run_tab_command('Close this tab group')
                closed=keep('close group');assert len(closed['groups'])==len(stacked['groups'])-1
                run_tab_command('Undo close tab')
                restored=keep('undo closed tab');assert len(restored['leaves'])==len(closed['leaves'])+1
                run_tab_command('Close all other tabs')
                only=keep('close other tabs');assert only['active']==restored['active']
                assert len(only['leaves'])<len(restored['leaves'])
                tab_click("[...document.querySelectorAll('.mod-root .workspace-tab-header')].find(e=>e.querySelector('.workspace-tab-header-inner-title')?.textContent.trim()==='Home')")
                current_pin=keep('select pinned');assert current_pin['active']=='Home.md'
                xdo('key','--clearmodifiers','ctrl+w');time.sleep(.5)
                pin_closed=keep('close pinned via shortcut')
                assert any(leaf['file']=='Home.md' and not leaf['pinned'] for leaf in pin_closed['leaves'])
                assert len(pin_closed['leaves'])==len(current_pin['leaves'])
                xdo('key','--clearmodifiers','ctrl+w');time.sleep(.5)
                unpinned_closed=keep('close unpinned via shortcut')
                assert not any(leaf['file']=='Home.md' for leaf in unpinned_closed['leaves'])
                assert len(unpinned_closed['leaves'])==len(pin_closed['leaves'])-1
                report['notes_unchanged']=original_notes=={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
                assert report['notes_unchanged'];report['status']='passed'
                print(json.dumps({'section':'tab-actions','status':'passed','actions':len(report['actions'])}))
            finally:
                (ROOT/('reports/obsidian-tab-windows-reference.json' if arguments.section=='tab-windows' else 'reports/obsidian-tab-actions-reference.json')).write_text(json.dumps(report,indent=2)+'\n')
            raise SystemExit(0)

        if arguments.section in ("recovery", "recovery-live"):
            report = {"appimage":str(arguments.appimage), "profile":"disposable", "status":"observed"}
            original_target = target
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def pages():
                with opener.open(f"http://127.0.0.1:{debug_port}/json/list",timeout=2) as response:
                    return [page for page in json.load(response) if page['type']=='page']
            def palette(name):
                xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2)
                xdo('type','--clearmodifiers','--delay','4',name);time.sleep(.2)
                xdo('key','Return');time.sleep(.4)
            def controls():
                return evaluate("JSON.stringify({body:document.body.innerText,controls:[...document.querySelectorAll('input,button,[aria-label]')].filter(e=>e.getBoundingClientRect().height).map(e=>{const r=e.getBoundingClientRect();return {tag:e.tagName,text:e.innerText,value:e.value,type:e.type,placeholder:e.placeholder,label:e.getAttribute('aria-label'),min:e.min,max:e.max,rect:[r.x,r.y,r.width,r.height]}})})")
            def click_dom(expression):
                get_rect="JSON.stringify((()=>{const e="+expression+";if(!e)return null;const r=e.getBoundingClientRect();return r.width&&r.height?[screenX+r.x+r.width/2,screenY+r.y+r.height/2]:null})())"
                rect=evaluate(get_rect)
                assert rect, 'Missing recovery control: '+expression
                time.sleep(.1)
                assert evaluate(get_rect)==rect, 'Recovery control moved before its click: '+expression
                xdo('mousemove',*[str(round(value)) for value in rect],'mousedown','1');time.sleep(.1);xdo('mouseup','1');time.sleep(.3)
            def wait(predicate,message):
                deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    if predicate():return
                    time.sleep(.1)
                raise AssertionError(message)
            try:
                original_notes={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
                report['commands'] = evaluate("JSON.stringify(Object.entries(app.commands.commands).filter(([id])=>id.startsWith('file-recovery')).map(([id,c])=>({id,name:c.name})))")
                assert len(report['commands'])==1
                palette(report['commands'][0]['name'])
                current_pages=pages()
                target=next((page for page in current_pages if page['id']!=original_target['id']),original_target)
                report['history_initial']=controls()
                assert {'Copy','Restore'} <= {c.get('text','') for c in report['history_initial']['controls']}, 'Actual history controls did not appear'
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-initial.png')
                home=vault/'Home.md'
                original=home.read_bytes()
                click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Copy')")
                copied=subprocess.check_output(['xclip','-selection','clipboard','-out'],env=environment)
                assert copied==original, 'History Copy changed the original Markdown bytes'
                report['copy_exact_bytes']=True
                xdo('key','Escape');time.sleep(.3)
                target=original_target
                palette('Open settings')
                deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    settings_pages=[page for page in pages() if page['id']!=original_target['id']]
                    if settings_pages:break
                    time.sleep(.1)
                assert settings_pages, 'Recovery audit settings window did not open'
                target=settings_pages[0]
                rect=evaluate("JSON.stringify((()=>{const e=[...document.querySelectorAll('.vertical-tab-nav-item')].find(e=>e.textContent.trim()==='File recovery');if(!e)return null;e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                assert rect, 'File recovery settings category missing'
                xdo('mousemove',*[str(round(value)) for value in rect],'click','1');time.sleep(.4)
                report['settings']=controls()
                report['settings_items']=evaluate("JSON.stringify([...document.querySelectorAll('.setting-item')].map(e=>({name:e.querySelector('.setting-item-name')?.innerText,description:e.querySelector('.setting-item-description')?.innerText,controls:[...e.querySelectorAll('input,button')].map(i=>({type:i.type,value:i.value,text:i.innerText,min:i.min,max:i.max}))})))")
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-settings.png')
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshot interval')?.querySelector('input')")
                xdo('key','--clearmodifiers','ctrl+a');xdo('type','--clearmodifiers','0');xdo('key','Tab');time.sleep(.4)
                report['interval_setting']=json.loads((settings/'file-recovery.json').read_text())
                settings_target=target
                target=original_target
                opened=evaluate("JSON.stringify(app.workspace.getLeavesOfType('markdown').map(leaf=>leaf.view.file?.path))")
                assert 'Template target.md' not in opened
                target=settings_target
                unopened=vault/'Template target.md'
                unopened_original=unopened.read_bytes()
                first_closed=b'# Closed note\n\nFIRST_CLOSED_SNAPSHOT\n'
                second_closed=b'# Closed note\n\nSECOND_CLOSED_VERSION_IS_DIFFERENT\n'
                unopened.write_bytes(first_closed);time.sleep(1.2)
                unopened.write_bytes(second_closed);time.sleep(1.2)
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                xdo('type','--clearmodifiers','Template target');time.sleep(.3);xdo('key','Return');time.sleep(.4)
                click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Copy')")
                assert subprocess.check_output(['xclip','-selection','clipboard','-out'],env=environment)==second_closed
                first_size=f'{len(first_closed)} B'
                click_dom("[...document.querySelectorAll('.modal div,.modal span')].find(e=>!e.children.length&&e.textContent.trim()==="+json.dumps(first_size)+")")
                click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Copy')")
                assert subprocess.check_output(['xclip','-selection','clipboard','-out'],env=environment)==first_closed
                assert unopened.read_bytes()==second_closed
                report['unopened_external_note']={'never_opened':True,'both_versions_exact':True,'history_browsing_preserved_current_bytes':True}
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-unopened.png')
                close="[...document.querySelectorAll('.modal.mod-sync-history .modal-header-button')].filter(e=>e.getBoundingClientRect().width&&e.getBoundingClientRect().height).at(-1)"
                click_dom(close)
                unopened.write_bytes(unopened_original);time.sleep(.5)
                home.write_bytes(original+b'\nRECOVERY_SECOND_VERSION\n')
                time.sleep(2)
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                xdo('type','--clearmodifiers','Home');time.sleep(.3);xdo('key','Return');time.sleep(.5)
                report['history_after_change']=controls()
                report['history_dom']=evaluate("JSON.stringify(document.querySelector('.file-recovery-text')?.closest('.modal')?.outerHTML||document.body.innerHTML.slice(-15000))")
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-versions.png')
                assert any(c.get('text')=='Restore' for c in report['history_after_change']['controls']), 'Choosing Home did not open its snapshots'
                size=f'{len(original)/1024:.2f} KB'
                click_dom("[...document.querySelectorAll('.modal div,.modal span')].find(e=>!e.children.length&&e.textContent.trim()==="+json.dumps(size)+")")
                click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Copy')")
                assert subprocess.check_output(['xclip','-selection','clipboard','-out'],env=environment)==original
                if arguments.section == "recovery-live":
                    def history_rows():
                        return evaluate("JSON.stringify([...document.querySelectorAll('.file-recovery-list-item-header')].map(e=>{const r=e.getBoundingClientRect();return {text:e.innerText,selected:e.classList.contains('is-active'),rect:[r.x,r.y,r.width,r.height]}}))")
                    before_live=history_rows()
                    third=original+b'\nRECOVERY_LIVE_ARRIVAL_WITH_A_DIFFERENT_SNAPSHOT_SIZE\n'
                    home.write_bytes(third);time.sleep(1.5)
                    after_live=history_rows()
                    click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Copy')")
                    assert subprocess.check_output(['xclip','-selection','clipboard','-out'],env=environment)==original
                    report['live_updates']={'before':before_live,'after':after_live,'list_unchanged':before_live==after_live,'selected_copy_unchanged':True}
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-live-arrival.png')
                    close="[...document.querySelectorAll('.modal.mod-sync-history .modal-header-button')].filter(e=>e.getBoundingClientRect().width&&e.getBoundingClientRect().height).at(-1)"
                    click_dom(close)
                    click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                    click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                    xdo('type','--clearmodifiers','Home');time.sleep(.3);xdo('key','Return');time.sleep(.5)
                    after_reopen=history_rows()
                    new_size=f'{len(third)/1024:.2f} KB'
                    assert any(new_size in row['text'] for row in after_reopen), 'Snapshot captured while history was open did not appear after reopening'
                    report['live_updates']['after_reopen']=after_reopen
                    click_dom("[...document.querySelectorAll('.modal div,.modal span')].find(e=>!e.children.length&&e.textContent.trim()==="+json.dumps(size)+")")
                    home.write_bytes(original)
                    assert original_notes=={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
                    report['notes_unchanged']=True
                    report['status']='passed'
                    print(json.dumps({'section':'recovery-live','status':report['status'],'live_list_unchanged':report['live_updates']['list_unchanged'],'before':len(before_live),'after':len(after_live),'reopened':len(after_reopen)}))
                    raise SystemExit(0)
                report['changes_before']=evaluate("JSON.stringify((()=>{const e=document.querySelector('input[type=checkbox]');return {checked:e.checked,html:e.parentElement.outerHTML}})())")
                click_dom("document.querySelector('input[type=checkbox]').closest('.checkbox-container')")
                report['changes_after']=evaluate("JSON.stringify((()=>{const e=document.querySelector('input[type=checkbox]');return {checked:e.checked,html:e.parentElement.outerHTML}})())")
                assert report['changes_after']!=report['changes_before'], 'Show changes did not change state with two saved versions'
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-changes.png')
                click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Restore')")
                report['restore_ui']=controls()
                wait(lambda:home.read_bytes()==original,'Restore did not recover exact snapshot bytes')
                report['copy_restore']={'copy_exact_bytes':True,'restored_exact_bytes':True,'show_changes_toggled':True}
                close="[...document.querySelectorAll('.modal.mod-sync-history .modal-header-button')].filter(e=>e.getBoundingClientRect().width&&e.getBoundingClientRect().height).at(-1)"
                if evaluate("JSON.stringify(Boolean("+close+"))"):click_dom(close)
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Clear history')?.querySelector('button')")
                report['clear_confirmation']=controls()
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-clear-confirm.png')
                click_dom("[...document.querySelectorAll('.modal button')].find(e=>e.textContent.trim()==='Cancel')")
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                xdo('type','--clearmodifiers','Home');time.sleep(.3);xdo('key','Return');time.sleep(.4)
                report['after_clear_cancel']=controls()
                assert any(c.get('text')=='Restore' for c in report['after_clear_cancel']['controls'])
                assert home.read_bytes()==original
                click_dom(close)
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Clear history')?.querySelector('button')")
                click_dom("[...document.querySelectorAll('.modal button')].filter(e=>e.textContent.trim()==='Clear').at(-1)")
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                xdo('type','--clearmodifiers','Home');time.sleep(.3);xdo('key','Return');time.sleep(.4)
                report['after_clear']=controls()
                assert not any(c.get('text')=='Restore' for c in report['after_clear']['controls'])
                assert original_notes=={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
                report['notes_unchanged']=True
                report['status']='passed'
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-after-clear.png')
                if evaluate("JSON.stringify(Boolean("+close+"))"):click_dom(close)
                home.write_bytes(original+b'\nAFTER_CLEAR_RECOVERY\n');time.sleep(1)
                home.write_bytes(original);time.sleep(1)
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                xdo('type','--clearmodifiers','Home');time.sleep(.3);xdo('key','Return');time.sleep(.4)
                home.rename(Path(temporary)/'deleted-Home.md')
                time.sleep(.7)
                click_dom("[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent==='Snapshots')?.querySelector('button')")
                click_dom("document.querySelector('input[placeholder=\"Choose a file...\"]')")
                xdo('type','--clearmodifiers','Home');time.sleep(.3);xdo('key','Return');time.sleep(.4)
                report['deleted_note_history']=controls()
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-deleted-note.png')
                click_dom("[...document.querySelectorAll('.modal div,.modal span')].find(e=>!e.children.length&&e.textContent.trim()==="+json.dumps(size)+")")
                click_dom("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Restore')")
                wait(lambda:home.exists() and home.read_bytes()==original,'Restore did not recreate a deleted note from its selected snapshot')
                report['deleted_note_recreated_exactly']=True
                report['dialogs_after_deleted_restore']=evaluate("JSON.stringify([...document.querySelectorAll('.modal')].map(e=>e.outerHTML))")
                assert original_notes=={p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-deleted-restored.png')
            except Exception as error:
                report.update(status='failed',error=str(error))
                ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-recovery-failure.png')
                raise
            finally:
                (ROOT/('reports/obsidian-recovery-live-reference.json' if arguments.section == 'recovery-live' else 'reports/obsidian-recovery-reference.json')).write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps({'section':'recovery','status':report['status']}))
            raise SystemExit(0)

        if arguments.section == "workspaces":
            report = {"appimage": str(arguments.appimage), "profile": "disposable", "status": "observed"}
            report["commands"] = evaluate("JSON.stringify(Object.entries(app.commands.commands).filter(([id])=>id.startsWith('workspaces:')).map(([id,c])=>({id,name:c.name})))")
            manage = next(c for c in report["commands"] if c["id"] == "workspaces:open-modal")
            xdo("key", "--clearmodifiers", "ctrl+p")
            time.sleep(.25)
            xdo("type", "--clearmodifiers", "--delay", "4", manage["name"])
            time.sleep(.25)
            xdo("key", "Return")
            time.sleep(.4)
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            with opener.open(f"http://127.0.0.1:{debug_port}/json/list", timeout=2) as response:
                pages = [page for page in json.load(response) if page["type"] == "page"]
            report["pages"] = [{key:page.get(key) for key in ("id","title","url")} for page in pages]
            target = next((page for page in pages if page["id"] != target["id"]), target)
            report["ui"] = evaluate("JSON.stringify({body:document.body.innerText,origin:[screenX,screenY],controls:[...document.querySelectorAll('input,button,[aria-label]')].filter(e=>e.getBoundingClientRect().height).map(e=>{const r=e.getBoundingClientRect();return {tag:e.tagName,text:e.innerText,value:e.value,placeholder:e.placeholder,label:e.getAttribute('aria-label'),classes:e.className,rect:[r.x,r.y,r.width,r.height]}})})")
            ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-workspaces-manager.png")
            report["actions"] = []
            def notes_hashes():
                return {p.relative_to(vault).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in vault.rglob('*.md')}
            before_notes = notes_hashes()
            workspace_file = settings / "workspaces.json"
            def wait(predicate, message):
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    if predicate(): return
                    time.sleep(.1)
                raise AssertionError(message)
            def click_dom(expression):
                rect = evaluate("JSON.stringify((()=>{const e=" + expression + ";if(!e)return null;const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                assert rect, f"Missing reference control: {expression}"
                xdo('mousemove', *[str(round(value)) for value in rect], 'click', '1')
                time.sleep(.2)
            def palette(name):
                xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2)
                xdo('type','--clearmodifiers','--delay','4',name);time.sleep(.2)
                xdo('key','Return');time.sleep(.3)
            def manager():
                palette(manage['name'])
                wait(lambda:evaluate("JSON.stringify(Boolean(document.querySelector('input[placeholder^=\"Save current workspace\"]')))"), 'Workspace manager did not open')
            def save_layout(name):
                click_dom("document.querySelector('input[placeholder^=\"Save current workspace\"]')")
                xdo('key','--clearmodifiers','ctrl+a')
                xdo('type','--clearmodifiers',name)
                click_dom("[...document.querySelectorAll('.modal button')].find(e=>e.textContent.trim()==='Save')")
                wait(lambda:workspace_file.exists() and name in json.loads(workspace_file.read_text()).get('workspaces',{}), 'Saved layout did not persist')
                report.setdefault('saved',{})[name] = json.loads(workspace_file.read_text())
                report['rows'] = evaluate("JSON.stringify([...document.querySelectorAll('.modal .list-item')].map(e=>({text:e.innerText,html:e.outerHTML})))")
                xdo('key','Escape');time.sleep(.2)
            def row_action(name, action):
                row = "[...document.querySelectorAll('.modal .list-item')].find(e=>e.textContent.startsWith(" + json.dumps(name) + "))"
                control = "(()=>{const row=" + row + ";if(!row)return null;return [...row.querySelectorAll('button,[aria-label]')].find(e=>" + ("e.textContent.trim()==='Load'" if action=='load' else "(e.getAttribute('aria-label')||'').includes('Delete')") + ")})()"
                click_dom(control)
            def state():
                return evaluate("JSON.stringify({active:app.workspace.getActiveFile()?.path,files:app.workspace.getLeavesOfType('markdown').map(leaf=>leaf.getViewState().state?.file).filter(Boolean),views:app.workspace.getLeavesOfType('markdown').map(leaf=>leaf.getViewState().state),loaded_files:app.workspace.getLeavesOfType('markdown').map(leaf=>leaf.view.file?.path).filter(Boolean),layout:app.workspace.getLayout()})")
            def open_first_steps(new_tab=True):
                if new_tab:xdo('key','--clearmodifiers','ctrl+t');time.sleep(.2)
                xdo('key','--clearmodifiers','ctrl+o');time.sleep(.2)
                xdo('type','--clearmodifiers','First steps');time.sleep(.2)
                xdo('key','Return');time.sleep(.3)
                wait(lambda:state().get('active')=='Notes/First steps.md', 'Quick switcher did not open the reference file')
            def compare_layout(expected):
                current = state()
                assert current['active']==expected['active'] and current['files']==expected['files'] and current['views']==expected['views'], (current['active'],current['files'],current['views'],expected['active'],expected['files'],expected['views'])
                for side in ('left','right'):
                    assert current['layout'][side].get('collapsed',False)==expected['layout'][side].get('collapsed',False)
                    assert current['layout'][side].get('width')==expected['layout'][side].get('width')
            try:
                writing = state()
                save_layout('Writing')
                report['actions'].append('Saved Writing from one Home tab and its sidebar state')
                manager()
                unchanged = workspace_file.read_bytes()
                click_dom("document.querySelector('input[placeholder^=\"Save current workspace\"]')")
                xdo('type','--clearmodifiers','Cancelled layout')
                xdo('key','Escape');time.sleep(.2)
                assert workspace_file.read_bytes()==unchanged
                report['actions'].append('Dismissed an unsaved layout name without changing the saved layouts')
                open_first_steps()
                if evaluate("JSON.stringify(app.workspace.activeLeaf.view.getMode())")=='preview':
                    xdo('key','--clearmodifiers','ctrl+e');time.sleep(.3)
                palette('Toggle left sidebar')
                reading = state()
                assert len(reading['files'])==2
                assert reading['layout']['left'].get('collapsed',False)!=writing['layout']['left'].get('collapsed',False)
                manager();save_layout('Reading')
                manager();row_action('Writing','load')
                wait(lambda:state().get('active')==writing['active'],'Writing did not restore its active note')
                compare_layout(writing)
                manager();row_action('Reading','load')
                wait(lambda:state().get('active')==reading['active'],'Reading did not restore its active note')
                compare_layout(reading)
                report['deferred_restore'] = state()
                click_dom("[...document.querySelectorAll('.workspace-split.mod-root .workspace-tab-header')].find(e=>e.querySelector('.workspace-tab-header-inner-title')?.textContent==='Home')")
                wait(lambda:state().get('active')=='Home.md','Restored inactive Home tab did not open')
                assert evaluate("JSON.stringify(document.querySelector('.markdown-preview-view')?.innerText||'')").find('A little room to think.')>=0
                click_dom("[...document.querySelectorAll('.workspace-split.mod-root .workspace-tab-header')].find(e=>e.querySelector('.workspace-tab-header-inner-title')?.textContent==='First steps')")
                wait(lambda:state().get('active')==reading['active'],'Restored second tab did not open')
                report['actions'].append('Loaded both named layouts with exact note order, active note, sidebar visibility and widths')
                report['command_flows'] = []
                for command_id in ('workspaces:save', 'workspaces:load', 'workspaces:save-and-load'):
                    entry = next(c for c in report['commands'] if c['id'] == command_id)
                    palette(entry['name'])
                    observed = evaluate("JSON.stringify({modal:document.querySelector('.modal')?.innerText||null,inputs:[...document.querySelectorAll('.modal input')].map(e=>({value:e.value,placeholder:e.placeholder})),suggestions:[...document.querySelectorAll('.suggestion-item')].map(e=>e.innerText)})")
                    report['command_flows'].append({'command':entry, 'ui':observed, 'stored':json.loads(workspace_file.read_text())})
                    ImageGrab.grab(xdisplay=display).save(ROOT / ('reports/obsidian-workspaces-' + command_id.split(':')[1] + '.png'))
                    xdo('key','Escape');time.sleep(.2)
                    compare_layout(reading)
                manager();row_action('Writing','delete')
                wait(lambda:'Writing' not in json.loads(workspace_file.read_text())['workspaces'],'Delete layout did not remove only its saved entry')
                xdo('key','Escape');time.sleep(.2)
                xdo('key','--clearmodifiers','ctrl+w');time.sleep(.3)
                palette('Toggle left sidebar')
                overwritten = state()
                manager();save_layout('Reading')
                assert list(json.loads(workspace_file.read_text())['workspaces'])==['Reading']
                open_first_steps()
                manager();row_action('Reading','load')
                wait(lambda:state().get('active')==overwritten['active'],'Overwritten layout did not restore the new active note')
                compare_layout(overwritten)
                manager();row_action('Reading','delete')
                wait(lambda:json.loads(workspace_file.read_text())['workspaces']=={},'Final layout deletion did not persist')
                assert notes_hashes()==before_notes
                report['actions'].append('Overwrote a named layout without duplicate entries and deleted layouts without changing any Markdown file')
                report.update(status='passed',writing=writing,reading=reading,overwritten=overwritten,notes_unchanged=True)
                ImageGrab.grab(xdisplay=display).save(ROOT / 'reports/obsidian-workspaces-complete.png')
            except Exception as error:
                report.update(status='failed',error=str(error),failure_ui=evaluate('JSON.stringify(document.body.innerText)'))
                ImageGrab.grab(xdisplay=display).save(ROOT / 'reports/obsidian-workspaces-failure.png')
                raise
            finally:
                (ROOT / "reports/obsidian-workspaces-reference.json").write_text(json.dumps(report,indent=2)+"\n")
            print(json.dumps({"section":"workspaces","commands":len(report["commands"]),"status":report["status"]}))
            raise SystemExit(0)
        if arguments.section in ("inventory", "note-creators", "unique-settings", "unique-links", "unique-links-configured"):
            report = evaluate("JSON.stringify({version:app.appVersion||app.version||null,plugins:Object.entries(app.internalPlugins.plugins).map(([id,p])=>({id,enabled:p.enabled,name:p.instance?.name})),commands:Object.entries(app.commands.commands).map(([id,c])=>({id,name:c.name,hotkeys:c.hotkeys})),settings:app.setting.settingTabs.map(t=>({id:t.id,name:t.name}))})")
            report.update({"appimage": str(arguments.appimage), "profile": "disposable", "actions": [], "status": "in_progress"})
            date_patterns = ["YYYY-MM-DD", "YYYY/MMMM/YYYY-MMM-DD", "YY Y YYYYYY", "M MM MMM MMMM Mo", "D DD Do DDD DDDD DDDo", "d dd ddd dddd do e E", "H HH h hh k kk", "m mm s ss", "S SS SSS SSSS SSSSSSSSS", "A a Z ZZ z zz", "Q Qo", "X x", "w ww wo gg gggg W WW Wo GG GGGG", "L LT LTS LL LLL LLLL", "l ll lll llll", "[Journal] YYYY [week] WW [مرحبا]", r"\Y YYYY [{{title}}]"]
            date_inputs = ["2026-10-21T13:05:09.123+03:30", "2024-02-29T00:00:00.007-05:00", "2023-12-31T23:59:59.999+00:00"]
            report["date_formats"] = evaluate("JSON.stringify(" + json.dumps(date_inputs) + ".map(source=>{const date=window.moment.parseZone(source).local().locale('en');return {source,input:date.format('YYYY-MM-DD[T]HH:mm:ss.SSSZ'),formats:" + json.dumps(date_patterns) + ".map(pattern=>({pattern,expected:date.format(pattern)}))}}))")
            output = ROOT / ("reports/obsidian-parity-reference.json" if arguments.section == "inventory" else f"reports/obsidian-{arguments.section}-reference.json")

            def persist():
                output.write_text(json.dumps(report, indent=2) + "\n")

            def wait_until(predicate, description):
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    try:
                        if predicate():
                            return
                    except (FileNotFoundError, json.JSONDecodeError):
                        # The app's settings write may still be in flight.
                        pass
                    time.sleep(.1)
                raise AssertionError(description)

            def command(prefix, text):
                entry = next(c for c in report["commands"] if c["name"].startswith(prefix) and text in c["name"].lower())
                xdo("key", "--clearmodifiers", "ctrl+p")
                time.sleep(.3)
                xdo("type", "--clearmodifiers", "--delay", "4", entry["name"])
                time.sleep(.25)
                assert evaluate("JSON.stringify(Boolean(document.querySelector('.prompt-input')))"), "Real command palette did not open"
                selected = evaluate("JSON.stringify(document.querySelector('.suggestion-item.is-selected')?.innerText||'')")
                normalized = lambda text: ''.join(c for c in text if c.isalnum()).casefold()
                assert normalized(entry["name"]) in normalized(selected), f"Command is unavailable in this state: {entry['name']}; selected {selected!r}"
                xdo("key", "Return")
                time.sleep(.35)
                return entry

            persist()
            try:
                if arguments.section.startswith("unique-links"):
                    originals={p.relative_to(vault).as_posix():p.read_bytes() for p in vault.rglob("*.md")}
                    note=vault/"Template target.md"
                    xdo("key","--clearmodifiers","ctrl+o");time.sleep(.2);xdo("type","--clearmodifiers","Template target");time.sleep(.2);xdo("key","Return");time.sleep(.3)
                    if evaluate("JSON.stringify(app.workspace.activeLeaf.view.getMode())")=="preview":xdo("key","--clearmodifiers","ctrl+e");time.sleep(.2)
                    position=evaluate("JSON.stringify((()=>{const r=document.querySelector('.cm-content').getBoundingClientRect();return [r.x+10,r.y+10]})())")
                    xdo("mousemove",*[str(round(value)) for value in position],"click","1");xdo("key","--clearmodifiers","ctrl+End")
                    if arguments.section == "unique-links-configured":
                        prefix=evaluate("JSON.stringify(window.moment().format('[Link-]YYYYMMDDHHmm'))")
                        seeded=vault/"Unique"/(prefix+".md");seeded.write_bytes(b"KEEP_EXISTING_LINK_TARGET\n")
                        wait_until(lambda:evaluate("JSON.stringify(Boolean(app.vault.getAbstractFileByPath("+json.dumps(seeded.relative_to(vault).as_posix())+")))"),"Reference did not notice its existing unique-link target")
                        report['collision_seed']={'path':seeded.relative_to(vault).as_posix(),'time_prefix':prefix}
                    before={p.relative_to(vault).as_posix() for p in vault.rglob("*.md")}
                    command("Unique note creator:","internal link")
                    wait_until(lambda:note.read_bytes()!=originals['Template target.md'],"Unique internal link did not edit the active document")
                    inserted=note.read_text();added={p.relative_to(vault).as_posix() for p in vault.rglob("*.md")}-before
                    assert inserted.startswith(originals['Template target.md'].decode()) and '[[' in inserted
                    assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)")=="Template target.md"
                    report['actions'].append({'workflow':'unique internal link','inserted':inserted,'created':[{'path':name,'text':(vault/name).read_text()} for name in sorted(added)],'selection':evaluate("JSON.stringify(app.workspace.activeLeaf.view.editor?.getSelection?.()||'')"),'cursor':evaluate("JSON.stringify(app.workspace.activeLeaf.view.editor?.getCursor?.()||null)")})
                    ImageGrab.grab(xdisplay=display).save(ROOT/f"reports/obsidian-{arguments.section}.png")
                    xdo("key","Escape");time.sleep(.2)
                    focused=evaluate("JSON.stringify(Boolean(document.activeElement?.closest('.cm-editor')))")
                    report['escape_focus']={'editor_focused':focused,'cursor':evaluate("JSON.stringify(app.workspace.activeLeaf.view.editor?.getCursor?.()||null)")}
                    assert focused,'Escape must preserve the actual editor focus'
                    xdo('type','--clearmodifiers','|LABEL')
                    wait_until(lambda:note.read_text()==inserted[:-2]+'|LABEL]]','Escape must preserve the caret before closing link markers')
                    report['escape_focus']['typed']=note.read_text()
                    xdo("key","--clearmodifiers","ctrl+z");wait_until(lambda:note.read_text()==inserted,"Label insertion did not undo exactly")
                    xdo("key","--clearmodifiers","ctrl+z");wait_until(lambda:note.read_bytes()==originals['Template target.md'],"Unique link did not undo as one edit")
                    xdo("key","--clearmodifiers","ctrl+shift+z");wait_until(lambda:note.read_text()==inserted,"Unique link did not redo exactly")
                    xdo("key","--clearmodifiers","ctrl+shift+z");wait_until(lambda:note.read_text()==inserted[:-2]+'|LABEL]]',"Label insertion did not redo exactly")
                    xdo("key","--clearmodifiers","ctrl+z");wait_until(lambda:note.read_text()==inserted,"Label undo did not restore the plain link")
                    xdo("key","--clearmodifiers","ctrl+z");wait_until(lambda:note.read_bytes()==originals['Template target.md'],"Reference source did not return to original bytes")
                    if arguments.section == "unique-links-configured":
                        saved_folder=base/"unavailable-unique-folder";(vault/"Unique").rename(saved_folder)
                        saved_template=base/"unavailable-template.md";(vault/"Templates/Meeting.md").rename(saved_template)
                        try:
                            wait_until(lambda:not evaluate("JSON.stringify(Boolean(app.vault.getAbstractFileByPath('Unique')))"),"Reference did not observe its removed settings folder")
                            command("Unique note creator:","internal link")
                            wait_until(lambda:note.read_bytes()!=originals['Template target.md'],"Unique-link insertion with inaccessible creator settings did not reach disk")
                            report['inaccessible_settings']={'text':note.read_text(),'ui':evaluate("JSON.stringify(document.body.innerText)"),'created':[p.relative_to(vault).as_posix() for p in vault.rglob('*.md') if p.relative_to(vault).as_posix() not in before]}
                            xdo("key","--clearmodifiers","ctrl+z");wait_until(lambda:note.read_bytes()==originals['Template target.md'],"Second unique-link action did not undo")
                        finally:
                            saved_folder.rename(vault/"Unique");saved_template.rename(vault/"Templates/Meeting.md")
                        assert seeded.read_bytes()==b"KEEP_EXISTING_LINK_TARGET\n"
                    xdo("key","Escape");xdo("key","--clearmodifiers","ctrl+e")
                    wait_until(lambda:evaluate("JSON.stringify(app.workspace.activeLeaf.view.getMode())")=="preview","Reference did not switch to Reading before availability checks")
                    name=next(c['name'] for c in report['commands'] if c['name'].startswith('Unique note creator:') and 'internal link' in c['name'].lower())
                    xdo("key","--clearmodifiers","ctrl+p");time.sleep(.2);xdo("type","--clearmodifiers","--delay","4",name);time.sleep(.2)
                    selected=evaluate("JSON.stringify(document.querySelector('.suggestion-item.is-selected')?.innerText||'')")
                    xdo("key","Return");time.sleep(.4)
                    report['reading']={'selected_command':selected,'text':note.read_text(),'mode':evaluate("JSON.stringify(app.workspace.activeLeaf.view.getMode())"),'palette_open':evaluate("JSON.stringify(Boolean(document.querySelector('.prompt-input')))"),'ui':evaluate("JSON.stringify(document.body.innerText)")}
                    if report['reading']['palette_open']:xdo("key","Escape")
                    assert note.read_bytes()==originals['Template target.md'],"Reading-state invocation unexpectedly changed the source; inspect before implementing"
                    assert all((vault/name).read_bytes()==data for name,data in originals.items())
                    report['status']='passed';persist();print(json.dumps({'section':arguments.section,'status':report['status'],'actions':report['actions'],'reading':report['reading']}))
                    raise SystemExit(0)
                if arguments.section == "unique-settings":
                    command("", "open settings")
                    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                    original_target = target
                    def settings_page():
                        with opener.open(f"http://127.0.0.1:{debug_port}/json/list", timeout=2) as response:
                            pages = json.load(response)
                        return next((page for page in pages if page["type"] == "page" and page["id"] != original_target["id"]), None)
                    wait_until(settings_page, "Separate settings window did not appear")
                    target = settings_page()
                    tab = "Unique note creator"
                    rect = evaluate("JSON.stringify((()=>{const e=[...document.querySelectorAll('.vertical-tab-nav-item')].find(e=>e.textContent.trim()===" + json.dumps(tab) + ");e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                    xdo("mousemove", *[str(round(value)) for value in rect], "click", "1")
                    time.sleep(.3)
                    report["settings"] = evaluate("JSON.stringify({active:document.querySelector('.vertical-tab-nav-item.is-active')?.innerText,items:[...document.querySelectorAll('.setting-item')].filter(e=>e.getBoundingClientRect().height).map(e=>({name:e.querySelector('.setting-item-name')?.innerText,description:e.querySelector('.setting-item-description')?.innerText,inputs:[...e.querySelectorAll('input')].map(i=>({type:i.type,value:i.value,placeholder:i.placeholder})),selects:[...e.querySelectorAll('select')].map(s=>({value:s.value,options:[...s.options].map(o=>({text:o.text,value:o.value}))}))}))})")
                    assert report["settings"]["active"] == tab
                    ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-unique-settings.png")
                    originals = {p.relative_to(vault).as_posix(): p.read_bytes() for p in vault.rglob("*.md")}
                    def click_element(expression):
                        rect = evaluate("JSON.stringify((()=>{const e=" + expression + ";if(!e)return null;const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                        assert rect, "Reference control is absent: " + expression
                        xdo("mousemove", *[str(round(value)) for value in rect], "click", "1")
                    def setting_control(name, kind):
                        return "[...document.querySelectorAll('.setting-item')].find(e=>e.querySelector('.setting-item-name')?.textContent===" + json.dumps(name) + ")?.querySelector(" + json.dumps(kind) + ")"
                    for name, value in (("New file location", "Unique"), ("Unique prefix format", "[Pair-]YYYYMMDDHHmm")):
                        expression = setting_control(name, "input")
                        click_element(expression)
                        xdo("key", "--clearmodifiers", "ctrl+a")
                        xdo("type", "--clearmodifiers", "--delay", "4", value)
                        xdo("key", "Tab")
                        assert evaluate("JSON.stringify((" + expression + ").value)") == value
                    report["template_control"] = evaluate("JSON.stringify((" + setting_control("Template file location", ".setting-item-control") + ").outerHTML)")
                    expression = setting_control("Template file location", ".combobox-button")
                    click_element(expression)
                    wait_until(lambda: evaluate("JSON.stringify(document.activeElement?.tagName==='INPUT'&&!document.activeElement.closest('.setting-item'))"), "Template chooser did not focus its query")
                    xdo("type", "--clearmodifiers", "--delay", "4", "Templates/Meeting")
                    time.sleep(.2)
                    report["template_choices"] = evaluate("JSON.stringify(document.body.innerText)")
                    ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-unique-template-chooser.png")
                    xdo("key", "Return")
                    wait_until(lambda: evaluate("JSON.stringify((" + expression + ").textContent)").strip().endswith("Meeting"), "Chosen template did not appear in its control")
                    wait_until(lambda: (settings / "zk-prefixer.json").exists(), "Unique-note settings were not saved")
                    report["configured"] = json.loads((settings / "zk-prefixer.json").read_text())
                    ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-unique-settings-configured.png")
                    click_element("document.querySelector('.titlebar-button.mod-close')")
                    wait_until(lambda: settings_page() is None, "Settings window did not close")
                    target = original_target
                    xdo("windowfocus", window)
                    before = {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")}
                    command("Unique note creator:", "unique")
                    report["missing_folder_ui"] = evaluate("JSON.stringify(document.body.innerText)")
                    time.sleep(.5)
                    assert {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")} == before
                    assert not (vault / "Unique").exists()
                    report["actions"].append({"workflow": "unique missing folder", "files_created": False, "originals_preserved": all((vault / name).read_bytes() == data for name, data in originals.items())})
                    (vault / "Unique").mkdir()
                    wait_until(lambda: evaluate("JSON.stringify(Boolean(app.vault.getAbstractFileByPath('Unique')))"), "Reference did not notice its new disposable folder")
                    def create_pair(pattern):
                        created = []
                        prefixes = []
                        second = evaluate("JSON.stringify(window.moment().seconds())")
                        if second > 50:
                            time.sleep(61 - second)
                        for index in range(2):
                            before = {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")}
                            prefixes.append(evaluate("JSON.stringify(window.moment().format(" + json.dumps(pattern) + "))"))
                            command("Unique note creator:", "unique")
                            if index == 1 and pattern in ("[Pair-00059]", "[Literal]"):
                                notice = evaluate("JSON.stringify(document.body.innerText)")
                                time.sleep(.4)
                                assert {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")} == before
                                assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)") is None
                                created.append({"path": None, "files_created": False, "ui": notice})
                                continue
                            if index == 0:
                                wait_until(lambda: len(list(vault.rglob("*.md"))) == len(before) + 1, "Unique command did not create its first file")
                            else:
                                wait_until(lambda: evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)") is not None, "Unique command did not open a file from its blank origin")
                            added = {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")} - before
                            assert len(added) == 1
                            name = added.pop()
                            assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)") == name
                            text = (vault / name).read_text()
                            assert name.startswith("Unique/") and "{{title}}" not in text
                            assert "# Meeting " + Path(name).stem in text and "{{content}}" in text
                            created.append({"path": name, "text": text})
                            if index == 0:
                                (vault / name).write_bytes(b"KEEP_CONFIGURED_FIRST\n")
                                time.sleep(.4)
                                xdo("key", "--clearmodifiers", "ctrl+t")
                                time.sleep(.2)
                                assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)") is None
                        assert (vault / created[0]["path"]).read_bytes() == b"KEEP_CONFIGURED_FIRST\n"
                        assert all((vault / name).read_bytes() == data for name, data in originals.items())
                        assert prefixes[0] == prefixes[1] and Path(created[0]["path"]).stem == prefixes[0]
                        report["actions"].append({"workflow": "configured unique notes", "format": pattern, "created": created, "time_prefixes": prefixes, "originals_preserved": True})
                        persist()
                        return created
                    created = create_pair("[Pair-]YYYYMMDDHHmm")
                    for pattern in ("[Pair-00059]", "[Literal]", "[Missing-]YYYYMMDDHHmmss"):
                        command("", "open settings")
                        wait_until(settings_page, "Separate settings window did not reopen")
                        target = settings_page()
                        assert evaluate("JSON.stringify(document.querySelector('.vertical-tab-nav-item.is-active')?.innerText)") == tab
                        expression = setting_control("Unique prefix format", "input")
                        click_element(expression)
                        xdo("key", "--clearmodifiers", "ctrl+a")
                        xdo("type", "--clearmodifiers", "--delay", "4", pattern)
                        xdo("key", "Tab")
                        wait_until(lambda: json.loads((settings / "zk-prefixer.json").read_text()).get("format") == pattern, "Changed unique format did not persist")
                        click_element("document.querySelector('.titlebar-button.mod-close')")
                        wait_until(lambda: settings_page() is None, "Reopened settings did not close")
                        target = original_target
                        xdo("windowfocus", window)
                        if pattern.startswith("[Missing-"):
                            template = vault / "Templates/Meeting.md"
                            saved_template = base / "saved-meeting-template.md"
                            template.rename(saved_template)
                            try:
                                wait_until(lambda: not evaluate("JSON.stringify(Boolean(app.vault.getAbstractFileByPath('Templates/Meeting.md')))"), "Reference did not observe the removed disposable template")
                                before = {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")}
                                command("Unique note creator:", "unique")
                                notice = evaluate("JSON.stringify(document.body.innerText)")
                                time.sleep(.4)
                                added = {p.relative_to(vault).as_posix() for p in vault.rglob("*.md")} - before
                                assert len(added) <= 1
                                report["actions"].append({"workflow": "unique missing template", "created": [{"path": name, "text": (vault / name).read_text()} for name in sorted(added)], "active": evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)"), "ui": notice})
                            finally:
                                saved_template.rename(template)
                            assert all((vault / name).read_bytes() == data for name, data in originals.items())
                            continue
                        create_pair(pattern)
                    ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-unique-configured-notes.png")
                    report["status"] = "passed"
                    persist()
                    print(json.dumps({"section": "unique-settings", "status": report["status"], "configured": report["configured"], "created": created}))
                    raise SystemExit(0)
                if arguments.section == "note-creators":
                    originals={p.relative_to(vault).as_posix():p.read_bytes() for p in vault.rglob('*.md')}
                    report['unique_defaults']=evaluate("JSON.stringify(app.internalPlugins.plugins['zk-prefixer']?.instance?.options||null)")
                    created=[]
                    prefixes=[]
                    second=evaluate('JSON.stringify(window.moment().seconds())')
                    if second>50:time.sleep(61-second)
                    for index in range(2):
                        before={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}
                        prefixes.append(evaluate("JSON.stringify(window.moment().format('YYYYMMDDHHmm'))"))
                        entry=command('Unique note creator:','unique')
                        wait_until(lambda:len(list(vault.rglob('*.md')))==len(before)+1,'Unique note command did not create exactly one new file')
                        added={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}-before
                        assert len(added)==1
                        name=added.pop();note=vault/name
                        assert note.read_bytes()==b''
                        assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)")==name
                        created.append(name)
                        if index==0:note.write_bytes(b'KEEP_UNIQUE_FIRST\n');time.sleep(.4)
                    assert (vault/created[0]).read_bytes()==b'KEEP_UNIQUE_FIRST\n'
                    assert prefixes[0]==prefixes[1] and Path(created[0]).stem==prefixes[0]
                    report['actions'].append({'workflow':'unique','created':created,'time_prefixes':prefixes,'same_prefix_collision_handled':True,'first_file_preserved':True,'second_file_empty':True,'defaults':report['unique_defaults']})
                    ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-unique-notes.png')
                    valid={p.relative_to(vault).as_posix() for p in vault.rglob('*.md')}
                    opened=[]
                    for _ in range(3):
                        xdo('key','--clearmodifiers','ctrl+t');time.sleep(.2)
                        assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)") is None
                        command('Random note:','random')
                        path=evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)")
                        assert path in valid
                        opened.append(path)
                    xdo('key','--clearmodifiers','ctrl+t');time.sleep(.2)
                    stash=base/'empty-vault-stash';stash.mkdir()
                    moved=[]
                    try:
                        for note in list(vault.rglob('*.md')):
                            relative=note.relative_to(vault);saved=stash/relative;saved.parent.mkdir(parents=True,exist_ok=True)
                            note.rename(saved);moved.append((note,saved))
                        wait_until(lambda:evaluate("JSON.stringify(app.vault.getMarkdownFiles().length)")==0,'Reference did not observe its empty disposable vault')
                        name=next(c['name'] for c in report['commands'] if c['id']=='random-note')
                        xdo('key','--clearmodifiers','ctrl+p');time.sleep(.2);xdo('type','--clearmodifiers','--delay','4',name);time.sleep(.2)
                        selected=evaluate("JSON.stringify(document.querySelector('.suggestion-item.is-selected')?.innerText||'')")
                        normalized=lambda text:''.join(c for c in text if c.isalnum()).casefold()
                        assert not selected or normalized(name) in normalized(selected)
                        xdo('key','Return');time.sleep(.3)
                        assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.file?.path||null)") is None
                        assert not list(vault.rglob('*.md'))
                        report['empty_random']={'selected_command':selected,'opened_file':None,'files_created':False,'ui':evaluate('JSON.stringify(document.body.innerText)')}
                        ImageGrab.grab(xdisplay=display).save(ROOT/'reports/obsidian-random-empty.png')
                        xdo('key','Escape')
                    finally:
                        for note,saved in moved:saved.rename(note)
                    assert all((vault/name).read_bytes()==data for name,data in originals.items())
                    assert (vault/created[0]).read_bytes()==b'KEEP_UNIQUE_FIRST\n' and (vault/created[1]).read_bytes()==b''
                    report['actions'].append({'workflow':'random','opened_from_blank':opened,'empty_vault_safe':True,'original_notes_unchanged':True})
                    report['status']='passed'
                    persist()
                    print(json.dumps({'section':'note-creators','status':'passed','created':created,'opened':opened}))
                    raise SystemExit(0)
                entry = command("Daily notes:", "today")
                wait_until(lambda: len(list((vault / "Daily").glob("*.md"))) == 1, "Daily-note command did not create a dated file")
                daily = next((vault / "Daily").glob("*.md"))
                daily_text = daily.read_text()
                assert daily_text.startswith("# " + daily.stem + "\n") and "- [ ] Review" in daily_text and "{{date" not in daily_text
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-parity-daily.png")
                entry = command("Daily notes:", "today")
                assert daily.read_text() == daily_text and len(list((vault / "Daily").glob("*.md"))) == 1
                report["actions"].append({"workflow": "daily", "command": entry, "path": daily.relative_to(vault).as_posix(), "text": daily_text, "existing_note_preserved": True})
                persist()
                xdo("key", "--clearmodifiers", "ctrl+o")
                time.sleep(.2)
                xdo("type", "--clearmodifiers", "Template target")
                time.sleep(.2)
                xdo("key", "Return")
                time.sleep(.3)
                mode = evaluate("JSON.stringify(app.workspace.activeLeaf.view.getMode())")
                if mode == "preview":
                    xdo("key", "--clearmodifiers", "ctrl+e")
                    time.sleep(.2)
                assert evaluate("JSON.stringify(app.workspace.activeLeaf.view.getMode())") == "source", "Template target is not editable"
                editor_rect = evaluate("JSON.stringify((()=>{const r=document.querySelector('.cm-content').getBoundingClientRect();return [r.x+10,r.y+10]})())")
                xdo("mousemove", *[str(round(value)) for value in editor_rect], "click", "1")
                xdo("key", "--clearmodifiers", "ctrl+End")
                entry = command("Templates:", "insert template")
                xdo("type", "--clearmodifiers", "Meeting")
                time.sleep(.2)
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-parity-template-chooser.png")
                xdo("key", "Return")
                note = vault / "Template target.md"
                wait_until(lambda: "# Meeting Template target" in note.read_text(), "Template selection did not insert into the active document")
                inserted = note.read_text()
                assert inserted.startswith("# Before\n") and "{{content}}" in inserted and "{{date" not in inserted
                xdo("key", "--clearmodifiers", "ctrl+z")
                wait_until(lambda: note.read_text() == "# Before\n\n", "Template insertion did not undo as one operation")
                xdo("key", "--clearmodifiers", "ctrl+shift+z")
                wait_until(lambda: note.read_text() == inserted, "Template redo did not restore the exact inserted source")
                xdo("key", "--clearmodifiers", "ctrl+Home")
                time.sleep(.3)
                xdo("key", "--clearmodifiers", "ctrl+e")
                time.sleep(.4)
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-parity-template-insert.png")
                report["actions"].append({"workflow": "templates", "command": entry, "text": inserted, "unknown_token_preserved": True, "single_undo_redo": True})
                command("", "open settings")
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                def settings_page():
                    with opener.open(f"http://127.0.0.1:{debug_port}/json/list", timeout=2) as response:
                        pages = json.load(response)
                    return next((page for page in pages if page["type"] == "page" and page["id"] != target["id"]), None)
                wait_until(settings_page, "Separate settings window did not appear")
                target = settings_page()
                time.sleep(.3)
                report["settings_ui"] = evaluate("JSON.stringify([...document.querySelectorAll('.vertical-tab-nav-item')].map(e=>({text:e.innerText,active:e.classList.contains('is-active')})))")
                report["settings_text"] = evaluate("JSON.stringify(document.querySelector('.modal.mod-settings')?.innerText||document.body.innerText)")
                assert len(report["settings_ui"]) >= 9, "Settings categories did not appear"
                version = re.search(r'(?:current\s+)?version\s*:?\s*(\d+\.\d+\.\d+)', report["settings_text"], re.I)
                if version:
                    report["version"] = version.group(1)
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-parity-settings.png")
                report["settings_sections"] = {}
                for tab in ("Daily notes", "Templates"):
                    rect = evaluate("JSON.stringify((()=>{const e=[...document.querySelectorAll('.vertical-tab-nav-item')].find(e=>e.textContent.trim()===" + json.dumps(tab) + ");e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect();return [screenX+r.x+r.width/2,screenY+r.y+r.height/2]})())")
                    xdo("mousemove", *[str(round(value)) for value in rect], "click", "1")
                    time.sleep(.3)
                    section = evaluate("JSON.stringify({active:document.querySelector('.vertical-tab-nav-item.is-active')?.innerText,items:[...document.querySelectorAll('.setting-item')].filter(e=>e.getBoundingClientRect().height).map(e=>({name:e.querySelector('.setting-item-name')?.innerText,description:e.querySelector('.setting-item-description')?.innerText,inputs:[...e.querySelectorAll('input')].map(i=>({type:i.type,value:i.value}))}))})")
                    assert section["active"] == tab, f"Wrong settings page after clicking {tab}: {section['active']}"
                    report["settings_sections"][tab] = section
                    ImageGrab.grab(xdisplay=display).save(ROOT / ("reports/obsidian-parity-settings-" + tab.lower().replace(' ', '-') + ".png"))
                report["status"] = "passed"
            except Exception as error:
                report["status"] = "failed"
                report["error"] = str(error)
                report["target_text"] = (vault / "Template target.md").read_text()
                report["failure_ui"] = evaluate("JSON.stringify({body:document.body.innerText,mode:globalThis.app?.workspace?.activeLeaf?.view?.getMode?.()})")
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-parity-failure.png")
                raise
            finally:
                persist()
            print(json.dumps({"version": report.get("version"), "plugins": len(report["plugins"]), "commands": len(report["commands"]), "actions": [a["workflow"] for a in report["actions"]], "status": report["status"]}))
            raise SystemExit(0)
        if arguments.section == "menus":
            def evaluate(expression):
                value = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], expression], text=True)
                return json.loads(value)

            row = evaluate("JSON.stringify((()=>{const e=document.querySelector('.nav-file-title');const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})())")
            xdo("mousemove", str(round(row["x"])), str(round(row["y"])), "click", "3")
            time.sleep(.3)
            report = evaluate("JSON.stringify({version:app.appVersion,items:[...document.querySelectorAll('.menu .menu-item')].map(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e);return {text:e.innerText,icon:e.querySelector('svg')?.outerHTML,font:s.fontSize,height:r.height,padding:s.padding};}),menu:[...document.querySelectorAll('.menu')].map(e=>({text:e.innerText,width:e.getBoundingClientRect().width,font:getComputedStyle(e).fontSize,padding:getComputedStyle(e).padding}))})")
            assert report["items"], "The reference file context menu did not open"
            (ROOT / "reports/obsidian-file-menu.json").write_text(json.dumps(report,indent=2)+"\n")
            ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-file-menu.png")
            print(json.dumps({"section":"menus","items":[item["text"] for item in report["items"]]}))
            raise SystemExit(0)
        if arguments.section == "hover":
            def evaluate(expression):
                value = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], expression], text=True)
                return json.loads(value)

            report = {}
            for name, selector in [("icon", ".side-dock-ribbon-action"), ("close", ".workspace-tab-header.is-active .workspace-tab-header-inner-close-button"), ("folder", ".nav-folder:not(.mod-root) > .nav-folder-title"), ("file", ".nav-file-title"), ("tab", ".workspace-tab-header.is-active")]:
                expression = "JSON.stringify((()=>{const e=[...document.querySelectorAll(" + json.dumps(selector) + ")].find(e=>e.getBoundingClientRect().width);if(!e)return null;const r=e.getBoundingClientRect(),s=getComputedStyle(e);return {rect:[r.x,r.y,r.right,r.bottom],background:s.backgroundColor,color:s.color,radius:s.borderRadius,opacity:s.opacity,cursor:s.cursor,transition:s.transition}})())"
                xdo("mousemove", "1000", "900")
                time.sleep(.2)
                normal = evaluate(expression)
                if not normal:
                    continue
                left, top, right, bottom = normal["rect"]
                xdo("mousemove", str(round((left+right)/2)), str(round((top+bottom)/2)))
                time.sleep(.3)
                report[name] = {"normal":normal, "hover":evaluate(expression)}
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports" / f"obsidian-hover-{name}.png")
            (ROOT / "reports/obsidian-hover.json").write_text(json.dumps(report, indent=2)+"\n")
            print(json.dumps(report))
            raise SystemExit(0)
        if arguments.section == "navigation":
            def evaluate(expression):
                value = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], expression], text=True)
                return json.loads(value)

            def open_reference(mode):
                evaluate("(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Notes/Navigation check.md',mode:" + json.dumps(mode) + "}}),JSON.stringify('opening'))")
                time.sleep(.8)
                if mode == "source":
                    xdo("key", "--clearmodifiers", "ctrl+Home")
                    time.sleep(.3)

            boxes = "JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active input[type=checkbox]')].filter(e=>e.getBoundingClientRect().width).map(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e),t=getComputedStyle(e.parentElement),a=getComputedStyle(e,'::after');return {checked:e.checked,x:r.x,y:r.y,width:r.width,height:r.height,radius:s.borderRadius,border:s.border,background:s.backgroundColor,marker:a.backgroundColor,margin:s.margin,font:t.fontSize,lineHeight:t.lineHeight,textColor:t.color,decoration:t.textDecorationLine,variables:Object.fromEntries(['--checkbox-size','--checkbox-radius','--checkbox-color','--checkbox-marker-color','--checklist-done-color','--checklist-done-decoration'].map(k=>[k,s.getPropertyValue(k)]))}}))"
            report = {"identity":evaluate("JSON.stringify({userAgent:navigator.userAgent})"), "cases":[]}
            report["link_hotkeys"] = evaluate("JSON.stringify(Object.entries(app.commands.commands).filter(([id])=>id.includes('link')).map(([id,c])=>({id,name:c.name,hotkeys:c.hotkeys,defaults:app.hotkeyManager.defaultKeys?.[id]})))")
            for mode in ("preview", "source"):
                open_reference(mode)
                report[mode + "_checkboxes"] = evaluate(boxes)
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports" / f"obsidian-navigation-{mode}.png")
                for modified in (False, True):
                    open_reference(mode)
                    link = evaluate("JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active a.internal-link,.workspace-leaf.mod-active .cm-link')].filter(e=>e.textContent==='Open target'&&e.getBoundingClientRect().width).map(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,html:e.outerHTML}}))")
                    assert link, f"No rendered link in {mode}"
                    bounds = link[0]
                    if modified:
                        xdo("keydown", "Control_L")
                    try:
                        xdo("mousemove", str(round(bounds["x"] + bounds["width"]/2)), str(round(bounds["y"] + bounds["height"]/2)), "click", "1")
                        time.sleep(.7)
                    finally:
                        if modified:
                            xdo("keyup", "Control_L")
                    state = evaluate("JSON.stringify({file:app.workspace.activeLeaf.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').length})")
                    report["cases"].append({"mode":mode,"ctrl":modified,"link":bounds,"result":state})
            evaluate("(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Notes/Navigation check.md',mode:'source',source:true}}),JSON.stringify('opening'))")
            time.sleep(.8)
            report["ctrl_enter_before"] = evaluate("JSON.stringify({file:app.workspace.activeLeaf.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').length})")
            evaluate("(()=>{const e=app.workspace.activeLeaf.view.editor;const i=e.getValue().indexOf('[[');if(i<0)throw Error('Missing source wiki link');e.setCursor(e.offsetToPos(i+5));e.focus();return JSON.stringify('ready')})()")
            xdo("key", "--clearmodifiers", "ctrl+Return")
            time.sleep(.7)
            report["ctrl_enter_after"] = evaluate("JSON.stringify({file:app.workspace.activeLeaf.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').length})")
            def delete_reference_current():
                before = evaluate("JSON.stringify({file:app.workspace.activeLeaf.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').length,type:app.workspace.activeLeaf.view.getViewType()})")
                evaluate("(()=>{if(app.vault.config)app.vault.config.trashOption='local';if(!app.commands.executeCommandById('app:delete-file'))throw Error('Missing delete command');return JSON.stringify('deleting')})()")
                time.sleep(.3)
                modal = evaluate("JSON.stringify([...document.querySelectorAll('.modal button')].map(b=>({text:b.textContent,classes:b.className})))")
                ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-delete-confirmation.png")
                confirmed = evaluate("(()=>{const b=[...document.querySelectorAll('.modal button')].find(b=>/delete|trash/i.test(b.textContent));if(b)b.click();return JSON.stringify(!!b)})()")
                time.sleep(.8)
                after = evaluate("JSON.stringify({file:app.workspace.activeLeaf.view.file?.path,tabs:app.workspace.getLeavesOfType('markdown').length,type:app.workspace.activeLeaf.view.getViewType()})")
                exists = evaluate("JSON.stringify(!!app.vault.getAbstractFileByPath(" + json.dumps(before["file"]) + "))")
                return {"before": before, "after": after, "modal_buttons": modal, "confirmed": confirmed, "file_still_exists": exists}
            report["delete_with_other_tabs"] = delete_reference_current()
            evaluate("(()=>{const active=app.workspace.activeLeaf;for(const leaf of app.workspace.getLeavesOfType('markdown'))if(leaf!==active)leaf.detach();return JSON.stringify('one tab')})()")
            time.sleep(.3)
            report["delete_last_tab"] = delete_reference_current()
            for case in (report["delete_with_other_tabs"], report["delete_last_tab"]):
                assert case["confirmed"] and not case["file_still_exists"], case
                assert case["after"]["tabs"] == case["before"]["tabs"] - 1, case
            assert report["delete_with_other_tabs"]["after"]["file"] == "Notes/Navigation check.md"
            assert report["delete_last_tab"]["after"]["type"] == "empty"
            (ROOT / "reports/obsidian-navigation.json").write_text(json.dumps(report,indent=2)+"\n")
            print(json.dumps(report))
            raise SystemExit(0)
        if arguments.section == "callouts":
            kinds = ["note", "abstract", "info", "todo", "tip", "success", "question", "warning", "failure", "danger", "bug", "example", "quote"]
            sample = (ROOT / "tests/fixtures/callouts.md").read_text()
            sample += "\n" + "\n\n".join(f"> [!{kind}] {kind.title()}" for kind in kinds) + "\n"
            (vault / "Callout checks.md").write_text(sample)
            time.sleep(.7)
            measure = """JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .callout')].map(e=>{
              const s=getComputedStyle(e),title=e.querySelector('.callout-title')||e,t=getComputedStyle(title),icon=e.querySelector('.callout-icon'),i=icon?getComputedStyle(icon):{},content=e.querySelector('.callout-content'),c=content?getComputedStyle(content):{};
              const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}};
              return {type:e.dataset.callout,fold:e.dataset.calloutFold,title:title.textContent,rect:rect(e),padding:[s.paddingTop,s.paddingRight,s.paddingBottom,s.paddingLeft],radius:s.borderRadius,background:s.backgroundColor,calloutColor:s.getPropertyValue('--callout-color'),titleStyle:{font:t.fontFamily,size:t.fontSize,weight:t.fontWeight,lineHeight:t.lineHeight,gap:t.gap,color:t.color,rect:rect(title)},icon:{width:i.width,height:i.height},body:{display:c.display,padding:[c.paddingTop,c.paddingRight,c.paddingBottom,c.paddingLeft],rect:rect(content)}}
            }))"""
            for appearance in ("dark", "light"):
                theme = "obsidian" if appearance == "dark" else "moonstone"
                switch = f"(app.vault.setConfig('theme','{theme}'),document.body.classList.remove('theme-dark','theme-light'),document.body.classList.add('theme-{appearance}'),'changed')"
                subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], switch], text=True)
                for mode in ("preview", "source"):
                    state = f"(app.workspace.activeLeaf.setViewState({{type:'markdown',state:{{file:'Callout checks.md',mode:'{mode}',source:false}}}}),'opening')"
                    subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], state], text=True)
                    time.sleep(.8)
                    measured = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], measure], text=True)
                    result = json.loads(measured)
                    assert result, f"No callouts rendered in {appearance}/{mode}"
                    (ROOT / f"reports/obsidian-callouts-{appearance}-{mode}.json").write_text(json.dumps(result, indent=2)+"\n")
                    ImageGrab.grab(xdisplay=display).save(ROOT / f"reports/obsidian-callouts-{appearance}-{mode}.png")
            print("Measured installed Obsidian callouts in both themes and views using a disposable profile.")
            raise SystemExit(0)
        fonts = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], query], text=True)
        (ROOT / "reports/obsidian-typography.json").write_text(json.dumps(json.loads(fonts), indent=2) + "\n")
        reading_mode = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Home.md',mode:'preview'}}), 'reading')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], reading_mode], text=True)
        time.sleep(.7)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-reading-reference.png")
        reading_fonts = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], query], text=True)
        (ROOT / "reports/obsidian-reading-typography.json").write_text(json.dumps(json.loads(reading_fonts), indent=2) + "\n")
        scalars = "---\nyes_word: yes\non_word: on\nflag: true\nleading_zero: 0123\ndate: 2026-10-20\n---\nBody\n"
        (vault / "YAML compatibility.md").write_text(scalars)
        time.sleep(1)
        yaml_query = "JSON.stringify(app.metadataCache.getFileCache(app.vault.getAbstractFileByPath('YAML compatibility.md')).frontmatter)"
        yaml = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], yaml_query], text=True)
        (ROOT / "reports/obsidian-yaml-reference.json").write_text(json.dumps(json.loads(yaml), indent=2) + "\n")
        print(xdo("getwindowname", window))
        properties = "---\naliases: [Notebook alias]\ntags: [notes/deep]\ndone: false\nscore: 3\nstatus: Active\n---\n\n# Properties GUI\nBody must stay exact.\n"
        (vault / "Properties GUI.md").write_text(properties)
        time.sleep(1)
        open_properties = "(app.workspace.getLeaf(false).openFile(app.vault.getAbstractFileByPath('Properties GUI.md'), {state:{mode:'source'}}), 'opening')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], open_properties], text=True)
        time.sleep(1)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-properties-reference.png")
        property_query = "JSON.stringify(['.metadata-container','.metadata-property','.metadata-property-icon','.metadata-property-key','.metadata-property-value','.metadata-add-button','.markdown-source-view.mod-cm6 .cm-content','.cm-contentContainer','.cm-scroller','.cm-sizer','.cm-editor','.inline-title','.cm-line'].map(selector=>{const e=document.querySelector(selector);if(!e)return {selector};const s=getComputedStyle(e);const r=e.getBoundingClientRect();return {selector,font:s.fontFamily,size:s.fontSize,color:s.color,x:r.x,y:r.y,width:r.width,height:r.height,minHeight:s.minHeight,padding:[s.paddingTop,s.paddingRight,s.paddingBottom,s.paddingLeft],margin:[s.marginTop,s.marginRight,s.marginBottom,s.marginLeft],gap:s.gap};}))"
        measured = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], property_query], text=True)
        (ROOT / "reports/obsidian-properties.json").write_text(json.dumps(json.loads(measured), indent=2) + "\n")
        (vault / "Assets").mkdir(exist_ok=True)
        reference_color = (63, 163, 122)
        Image.new("RGB", (600, 240), reference_color).save(vault / "Assets/Attachment.png")
        (vault / "Image GUI.md").write_text("# Image GUI\n\n![[Assets/Attachment.png|180]]\n\nAfter image.\n")
        time.sleep(1)
        open_image = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Image GUI.md',mode:'source'}}), 'image note')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], open_image], text=True)
        time.sleep(.7)
        lines_query = "JSON.stringify(Array.from(document.querySelectorAll('.workspace-leaf.mod-active .cm-line')).map(e=>{const s=getComputedStyle(e);const r=e.getBoundingClientRect();return {text:e.textContent,x:r.x,y:r.y,width:r.width,height:r.height,size:s.fontSize,lineHeight:s.lineHeight,padding:[s.paddingTop,s.paddingBottom],margin:[s.marginTop,s.marginBottom]};}))"
        line_metrics = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], lines_query], text=True)
        (ROOT / "reports/obsidian-live-image-lines.json").write_text(json.dumps(json.loads(line_metrics),indent=2)+"\n")
        frame = ImageGrab.grab(xdisplay=display).convert("RGB")
        frame.save(ROOT / "reports/obsidian-live-image-reference.png")
        first_row = next(y for y in range(frame.height) if sum(frame.getpixel((x,y)) == reference_color for x in range(400,1350)) > 50)
        columns = [x for x in range(400,1350) if frame.getpixel((x, first_row+10)) == reference_color]
        xdo("mousemove", str((min(columns)+max(columns))//2), str(first_row+20), "click", "1")
        time.sleep(.4)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-live-image-click-reference.png")
        image_query = "JSON.stringify({type:app.workspace.activeLeaf.view.getViewType(),state:app.workspace.activeLeaf.view.getState(),modals:document.querySelectorAll('.modal-container').length,selections:app.workspace.activeLeaf.view.editor.listSelections()})"
        clicked = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], image_query], text=True)
        (ROOT / "reports/obsidian-image-click.json").write_text(json.dumps(json.loads(clicked),indent=2)+"\n")
        source_mode = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Home.md',mode:'source',source:true}}), 'source')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], source_mode], text=True)
        time.sleep(.7)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-source-reference.png")
        source_query = "JSON.stringify({state:app.workspace.activeLeaf.view.getState(),lines:[...document.querySelectorAll('.workspace-leaf.mod-active .cm-line')].map(e=>{const s=getComputedStyle(e);const r=e.getBoundingClientRect();return {text:e.textContent,font:s.fontFamily,color:s.color,size:s.fontSize,lineHeight:s.lineHeight,x:r.x,y:r.y,width:r.width,height:r.height,padding:[s.paddingTop,s.paddingBottom],fragments:[...e.children].map(n=>{const t=getComputedStyle(n);const q=n.getBoundingClientRect();return {text:n.textContent,x:q.x,width:q.width,size:t.fontSize,font:t.fontFamily,color:t.color,weight:t.fontWeight,letterSpacing:t.letterSpacing,decoration:t.textDecoration,display:t.display,opacity:t.opacity}})};})})"
        measured_source = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], source_query], text=True)
        (ROOT / "reports/obsidian-source-typography.json").write_text(json.dumps(json.loads(measured_source),indent=2)+"\n")
        live_mode = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Home.md',mode:'source',source:false}}), 'live')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], live_mode], text=True)
        time.sleep(.7)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-live-home-reference.png")
        measured_live = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], source_query], text=True)
        (ROOT / "reports/obsidian-live-home-typography.json").write_text(json.dumps(json.loads(measured_live),indent=2)+"\n")
        scrolled = "(document.querySelector('.workspace-leaf.mod-active .cm-scroller').scrollTop += 350, 'scrolled')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], scrolled], text=True)
        time.sleep(.3)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-live-setext-reference.png")
        visibility_query = "JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .cm-line')].filter(e=>e.textContent.trim()==='---'||e.textContent.startsWith('> ')).map(e=>{const r=e.getBoundingClientRect();const s=getComputedStyle(e);return {text:e.textContent,y:r.y,height:r.height,display:s.display,visibility:s.visibility,opacity:s.opacity,children:[...e.children].map(n=>{const t=getComputedStyle(n);const q=n.getBoundingClientRect();return {text:n.textContent,color:t.color,fontSize:t.fontSize,display:t.display,visibility:t.visibility,opacity:t.opacity,width:q.width,height:q.height}})}}))"
        inactive_visibility = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], visibility_query], text=True)
        (ROOT / "reports/obsidian-live-markers.json").write_text(json.dumps(json.loads(inactive_visibility),indent=2)+"\n")
        quote_cursor = "(app.workspace.activeLeaf.view.editor.setCursor({line:app.workspace.activeLeaf.view.editor.getValue().split('\\n').findIndex(line=>line.startsWith('> ')),ch:5}), 'quote cursor')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], quote_cursor], text=True)
        time.sleep(.3)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-live-quote-reference.png")
        quote_visibility = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], visibility_query], text=True)
        (ROOT / "reports/obsidian-active-markers.json").write_text(json.dumps(json.loads(quote_visibility),indent=2)+"\n")
        typography = "> Quoted **warm** body.\n> > Nested body.\n\nHow are you?!\n---\n\n- First row\n- Second row\n\n```text\n> literal\n```\n"
        (vault / "Typography GUI.md").write_text(typography)
        time.sleep(.6)
        typography_mode = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Typography GUI.md',mode:'source',source:false}}), 'typography')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], typography_mode], text=True)
        time.sleep(.7)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-typography-live.png")
        block_query = "JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .cm-line')].map(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e);return {text:e.textContent,classes:e.className,x:r.x,y:r.y,width:r.width,height:r.height,color:s.color,background:s.backgroundColor,size:s.fontSize,font:s.fontFamily,lineHeight:s.lineHeight,padding:[s.paddingTop,s.paddingRight,s.paddingBottom,s.paddingLeft],children:[...e.children].map(n=>{const q=n.getBoundingClientRect(),t=getComputedStyle(n);return {text:n.textContent,classes:n.className,x:q.x,y:q.y,width:q.width,height:q.height,size:t.fontSize,color:t.color,background:t.backgroundColor,display:t.display,visibility:t.visibility}})}}))"
        block_metrics = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], block_query], text=True)
        (ROOT / "reports/obsidian-typography-live.json").write_text(json.dumps(json.loads(block_metrics),indent=2)+"\n")
        code_cursor = "(app.workspace.activeLeaf.view.editor.setCursor({line:10,ch:5}), 'code cursor')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], code_cursor], text=True)
        time.sleep(.3)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-typography-code-active.png")
        typography_source = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Typography GUI.md',mode:'source',source:true}}), 'typography source')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], typography_source], text=True)
        time.sleep(.3)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-typography-source.png")
        source_blocks = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], block_query], text=True)
        (ROOT / "reports/obsidian-typography-source.json").write_text(json.dumps(json.loads(source_blocks),indent=2)+"\n")
        rust_sample = (ROOT / "tests/fixtures/syntax.md").read_text()
        (vault / "Syntax GUI.md").write_text(rust_sample)
        time.sleep(.6)
        rust_mode = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Syntax GUI.md',mode:'source',source:false}}), 'syntax')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], rust_mode], text=True)
        time.sleep(.5)
        syntax_metrics = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], block_query], text=True)
        (ROOT / "reports/obsidian-syntax-live.json").write_text(json.dumps(json.loads(syntax_metrics),indent=2)+"\n")
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-syntax-live.png")
        light_mode = "(document.body.classList.remove('theme-dark'),document.body.classList.add('theme-light'),'light palette')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], light_mode], text=True)
        time.sleep(.3)
        light_metrics = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], block_query], text=True)
        (ROOT / "reports/obsidian-syntax-light.json").write_text(json.dumps(json.loads(light_metrics),indent=2)+"\n")
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-syntax-light.png")
        dark_mode = "(document.body.classList.remove('theme-light'),document.body.classList.add('theme-dark'),'dark palette')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], dark_mode], text=True)
        rust_reading = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Syntax GUI.md',mode:'preview'}}), 'reading code')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], rust_reading], text=True)
        time.sleep(.5)
        reading_code_query = "JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view pre')].map(e=>{const info=n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);return {text:n.textContent,classes:n.className,x:r.x,y:r.y,width:r.width,height:r.height,color:s.color,background:s.backgroundColor,size:s.fontSize,font:s.fontFamily,lineHeight:s.lineHeight,padding:[s.paddingTop,s.paddingRight,s.paddingBottom,s.paddingLeft],margin:[s.marginTop,s.marginBottom],radius:s.borderRadius,display:s.display,opacity:s.opacity}};return {panel:info(e),code:info(e.querySelector('code')),tokens:[...e.querySelectorAll('span')].map(info),controls:[...e.querySelectorAll('button')].map(info)}}))"
        for appearance, switch in [("dark", dark_mode), ("light", light_mode)]:
            subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], switch], text=True)
            time.sleep(.3)
            measured_code = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], reading_code_query], text=True)
            (ROOT / f"reports/obsidian-reading-code-{appearance}.json").write_text(json.dumps(json.loads(measured_code),indent=2)+"\n")
            ImageGrab.grab(xdisplay=display).save(ROOT / f"reports/obsidian-reading-code-{appearance}.png")
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], dark_mode], text=True)
        (vault / "Table GUI.md").write_text((ROOT / "tests/fixtures/table.md").read_text())
        time.sleep(.6)
        table_reading = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Table GUI.md',mode:'preview'}}), 'reading table')"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], table_reading], text=True)
        time.sleep(.5)
        table_query = "JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view table')].map(e=>{const info=n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);return {text:n.textContent,tag:n.tagName,classes:n.className,x:r.x,y:r.y,width:r.width,height:r.height,color:s.color,background:s.backgroundColor,size:s.fontSize,font:s.fontFamily,weight:s.fontWeight,spacing:s.letterSpacing,feature:s.fontFeatureSettings,variation:s.fontVariationSettings,lineHeight:s.lineHeight,align:s.textAlign,border:s.border,borderColor:s.borderColor,borderCollapse:s.borderCollapse,padding:[s.paddingTop,s.paddingRight,s.paddingBottom,s.paddingLeft],margin:[s.marginTop,s.marginBottom]}};return {table:info(e),headings:[...document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view h2')].map(info),rows:[...e.rows].map(r=>[...r.cells].map(n=>({cell:info(n),children:[...n.children].map(info)})))}}))"
        for appearance, switch in [("dark", dark_mode), ("light", light_mode)]:
            subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], switch], text=True)
            time.sleep(.3)
            measured_table = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], table_query], text=True)
            (ROOT / f"reports/obsidian-reading-table-{appearance}.json").write_text(json.dumps(json.loads(measured_table),indent=2)+"\n")
            ImageGrab.grab(xdisplay=display).save(ROOT / f"reports/obsidian-reading-table-{appearance}.png")
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], dark_mode], text=True)
        (vault / "Persian GUI.md").write_text((ROOT / "tests/fixtures/persian.md").read_text())
        time.sleep(.6)
        rtl_query = "JSON.stringify([...document.querySelectorAll('.workspace-leaf.mod-active .cm-line, .workspace-leaf.mod-active .markdown-preview-view p, .workspace-leaf.mod-active .markdown-preview-view h2')].map(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e);return {text:e.textContent,x:r.x,y:r.y,width:r.width,height:r.height,font:s.fontFamily,size:s.fontSize,lineHeight:s.lineHeight,direction:s.direction,align:s.textAlign,bidi:s.unicodeBidi}}))"
        for view, state in [("source", "mode:'source',source:true"), ("live", "mode:'source',source:false"), ("reading", "mode:'preview'")]:
            rtl_mode = "(app.workspace.activeLeaf.setViewState({type:'markdown',state:{file:'Persian GUI.md'," + state + "}}), 'Persian reference')"
            subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], rtl_mode], text=True)
            time.sleep(.4)
            rtl_metrics = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], rtl_query], text=True)
            (ROOT / f"reports/obsidian-persian-{view}.json").write_text(json.dumps(json.loads(rtl_metrics),indent=2)+"\n")
            ImageGrab.grab(xdisplay=display).save(ROOT / f"reports/obsidian-persian-{view}.png")
        for index in range(10):
            (vault / f"Overflow{index:02}.md").write_text(f"# Overflow{index:02}\n\nExact tab body {index}.\n")
        time.sleep(.6)
        tabs_mode = "(async()=>{for(let i=0;i<10;i++){await app.workspace.getLeaf('tab').setViewState({type:'markdown',state:{file:'Overflow'+String(i).padStart(2,'0')+'.md',mode:'source',source:false}})}})()"
        subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], tabs_mode], text=True)
        time.sleep(1)
        ImageGrab.grab(xdisplay=display).save(ROOT / "reports/obsidian-tab-overflow.png")
        tabs_query = "JSON.stringify([...document.querySelectorAll('.workspace-tabs.mod-active .workspace-tab-header')].map(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e),n=e.querySelector('.workspace-tab-header-inner-close-button');return {title:e.textContent,active:e.classList.contains('is-active'),x:r.x,y:r.y,width:r.width,height:r.height,minWidth:s.minWidth,maxWidth:s.maxWidth,closeOpacity:n?getComputedStyle(n).opacity:null}}))"
        tabs_metrics = subprocess.check_output(["node", "--input-type=module", "-e", code, target["webSocketDebuggerUrl"], tabs_query], text=True)
        (ROOT / "reports/obsidian-tab-overflow.json").write_text(json.dumps(json.loads(tabs_metrics),indent=2)+"\n")
    finally:
        os.killpg(app.pid, signal.SIGTERM)
        app.wait(timeout=10)
        server.terminate()
        server.wait(timeout=5)
        log.close()
