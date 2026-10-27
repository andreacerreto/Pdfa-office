"""End-to-end CLI tests with real Ghostscript and veraPDF, not mocked engines.

Usage: python tests/test_e2e.py path/to/pdfa-office-cli [output-directory]
Point PDFA_OFFICE_* at installed runtimes when testing outside the installer.
Generate fixtures first with tests/make_fixtures.py. Dependencies: pypdf.
"""
from pathlib import Path
from hashlib import sha256
from xml.etree import ElementTree as ET
import json
import os
import subprocess
import sys
import time


def digest(path):
    return sha256(path.read_bytes()).hexdigest()


def main():
    binary = Path(sys.argv[1]).resolve()
    root = Path(sys.argv[2] if len(sys.argv) > 2 else 'test-output').resolve()
    fixtures = root / 'fixtures'
    outputs = root / 'converted'
    outputs.mkdir(parents=True, exist_ok=True)
    before = {p.name: digest(p) for p in fixtures.glob('*.pdf')}
    observations = []
    results = []

    def convert(name, standard, folder=outputs, extra=()):
        started = time.monotonic()
        command = [str(binary), 'convert', '--format', standard, '--output', str(folder), '--json', *extra, str(fixtures/name)]
        completed = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=120)
        try:
            body = json.loads(completed.stdout)
        except Exception:
            raise AssertionError(f'CLI returned no JSON: {completed.returncode}\n{completed.stdout}\n{completed.stderr}')
        observations.append({'input':name,'standard':standard,'exit_code':completed.returncode,'seconds':round(time.monotonic()-started,2),'result':body})
        return completed, body[0]

    for standard in ('1b','2b','3b'):
        for name in ('documento_ufficio.pdf','trasparenze.pdf','scansione.pdf'):
            proc, record = convert(name,standard)
            assert proc.returncode == 0, proc.stderr
            assert record['status']=='conforme'
            result=record['result']; results.append(result)
            receipt=json.loads(Path(result['receipt_path']).read_text(encoding='utf-8'))
            assert receipt['input']['sha256']==before[name]
            assert receipt['output']['sha256']==digest(Path(result['output_path']))
            assert receipt['validation']['is_compliant'] is True
            assert receipt['validation']['report_sha256']==digest(Path(result['report_path']))
            xml=ET.parse(result['report_path'])
            report=xml.find('./jobs/job/validationReport')
            assert report.attrib['isCompliant']=='true'
            assert report.attrib['profileName'].lower()==f'pdf/a-{standard} validation profile'
            print(f'PASS conversion {name} -> {standard}',flush=True)

    for name in ('pagina_ruotata.pdf','società à & (bilancio) 100%.pdf'):
        proc,record=convert(name,'2b')
        assert proc.returncode==0,proc.stderr
        results.append(record['result'])
        print(f'PASS paths/rotation {name}',flush=True)

    first=next(r for r in results if Path(r['input_path']).name=='documento_ufficio.pdf' and r['standard']=='2b')
    old_hash=digest(Path(first['output_path']))
    proc,record=convert('documento_ufficio.pdf','2b')
    assert proc.returncode==0,proc.stderr
    assert record['result']['output_path'] != first['output_path']
    assert digest(Path(first['output_path'])) == old_hash
    print('PASS no-overwrite collision',flush=True)

    for name in ('protetto.pdf','con_allegato.pdf','campo_firma_popolato.pdf','non_valido.pdf'):
        folder=root/'rejections'/Path(name).stem
        proc,record=convert(name,'2b',folder)
        assert proc.returncode==1,proc.stderr
        assert record['status']=='errore'
        assert not list(folder.glob('*.pdf'))
        assert not list(folder.glob('*.verapdf.xml'))
        print(f'PASS expected refusal {name}',flush=True)

    proc,record=convert('campo_firma_popolato.pdf','2b',root/'signed-copy',('--allow-signed-copy',))
    assert proc.returncode==0,proc.stderr
    assert any('firma' in warning for warning in record['result']['warnings'])
    print('PASS explicit signed-copy consent',flush=True)
    after={p.name:digest(p) for p in fixtures.glob('*.pdf')}
    assert before==after,'An original was modified'
    assert not list(root.rglob('.pdfa-office-*')),'Temporary job files not cleaned'
    assert not list(root.rglob('*.pdfa-office-lock')),'Output locks not cleaned'
    print('PASS original hashes and temporary cleanup',flush=True)
    (root/'e2e-results.json').write_text(json.dumps({'observations':observations,'originals_unchanged':True,'successful_conversions':len([o for o in observations if o['exit_code']==0]),'expected_rejections':len([o for o in observations if o['exit_code']==1])},ensure_ascii=False,indent=2),encoding='utf-8')


if __name__=='__main__':
    main()
