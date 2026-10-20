"""Capture the installed Obsidian using a disposable profile and sample vault."""
import argparse
import json
import os
from pathlib import Path
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
parser.add_argument("--section", choices=["all", "callouts"], default="all")
arguments = parser.parse_args()
with tempfile.TemporaryDirectory(prefix="rust-notes-reference-") as temporary:
    base = Path(temporary)
    vault = base / "Reference Vault"
    shutil.copytree(ROOT / "demo-vault", vault)
    settings = vault / ".obsidian"
    settings.mkdir(exist_ok=True)
    original = Path("/home/masih/Desktop/o/Main Vault/.obsidian")
    appearance = json.loads((original / "appearance.json").read_text())
    appearance["theme"] = "obsidian"
    (settings / "appearance.json").write_text(json.dumps(appearance))
    shutil.copytree(original / "snippets", settings / "snippets", dirs_exist_ok=True)
    (settings / "app.json").write_text(json.dumps({"defaultViewMode": "preview"}))
    profile = base / "profile"
    profile.mkdir()
    (profile / "obsidian.json").write_text(json.dumps({"vaults": {"0123456789abcdef": {"path": str(vault), "ts": 1, "open": True}}}))
    read_fd, write_fd = os.pipe()
    server = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1440x960x24", "-nolisten", "tcp", "-ac"], pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.close(write_fd)
    display = ":" + os.read(read_fd, 64).decode().strip()
    os.close(read_fd)
    environment = {**os.environ, "DISPLAY": display, "WAYLAND_DISPLAY": "", "ELECTRON_OZONE_PLATFORM_HINT": "x11"}
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
    app = subprocess.Popen(["/home/masih/Desktop/o/Obsidian.AppImage", "--no-sandbox", f"--user-data-dir={profile}", "--ozone-platform=x11", f"--remote-debugging-port={debug_port}"], env=environment, stdout=log, stderr=log, start_new_session=True)
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
