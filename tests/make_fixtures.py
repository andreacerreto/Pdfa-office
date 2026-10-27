"""Generate synthetic office PDFs for end-to-end tests; no real personal data."""
from pathlib import Path
import io
import sys
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor, Color, white
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.lib.utils import ImageReader
from pypdf import PdfReader, PdfWriter
from pypdf.generic import DictionaryObject, NameObject, ArrayObject, NumberObject, ByteStringObject
from PIL import Image, ImageDraw, ImageFont


def make(destination: Path):
    destination.mkdir(parents=True, exist_ok=True)
    font = Path('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf')
    if not font.is_file():
        font = Path('C:/Windows/Fonts/arial.ttf')
    if not font.is_file():
        raise SystemExit('Provide DejaVu Sans on Linux or Arial on Windows for fixtures.')
    pdfmetrics.registerFont(TTFont('TestSans', str(font)))
    path = destination / 'documento_ufficio.pdf'
    c = canvas.Canvas(str(path), pagesize=(595.276, 841.89), pageCompression=1)
    c.setTitle('Documento sintetico di collaudo PDF/A Office')
    for page in range(1, 3):
        c.setFillColor(HexColor('#153452'))
        c.rect(0, 728, 596, 114, fill=1, stroke=0)
        c.setFillColor(white)
        c.setFont('TestSans', 23)
        c.drawString(42, 780, 'Documento di ufficio')
        c.setFont('TestSans', 11)
        c.drawString(42, 753, 'Collaudo sintetico | nessun dato personale')
        c.setFillColor(HexColor('#20354A'))
        c.setFont('TestSans', 12)
        lines = [
            'Oggetto: verifica conversione e archiviazione.',
            'Caratteri: à è é ì ò ù - euro € - società - più.',
            'Importi: 1.234,56 €; 987,65 €; totale 2.222,21 €.',
            'Questa riga deve restare leggibile e selezionabile.',
        ]
        for i, line in enumerate(lines):
            c.drawString(42, 687-i*27, line)
        for row, (label, amount) in enumerate([('Consulenza', '1.234,56'), ('Servizi', '987,65'), ('Totale', '2.222,21')]):
            y=499-row*38
            c.setFillColor(HexColor('#E8EFF5') if row%2==0 else HexColor('#F4F7FA'))
            c.rect(42,y,511,36,fill=1,stroke=0)
            c.setFillColor(HexColor('#20354A'))
            c.drawString(53,y+12,label)
            c.drawRightString(542,y+12,amount+' €')
        c.setFont('TestSans', 10)
        c.drawString(42,52,'Solo materiale di test - PDF/A Office')
        c.drawRightString(552,52,f'Pagina {page} di 2')
        c.showPage()
    c.save()

    path = destination / 'trasparenze.pdf'
    c = canvas.Canvas(str(path), pagesize=(595.276,841.89),pdfVersion=(1,4))
    c.setFont('TestSans',20)
    c.drawString(42,780,'Trasparenze e grafica vettoriale')
    c.setFillColor(HexColor('#0068D9'))
    c.circle(235,580,125,stroke=0,fill=1)
    c.setFillColor(HexColor('#F25757'))
    c.setFillAlpha(.45)
    c.rect(210,450,245,220,stroke=0,fill=1)
    c.setFillAlpha(1)
    c.setFillColor(HexColor('#20354A'))
    c.setFont('TestSans',12)
    c.drawString(42,350,'Verificare i colori della sovrapposizione.')
    c.showPage(); c.save()

    img = Image.new('RGB',(1190,1684),'#fffdf8')
    d = ImageDraw.Draw(img)
    f = ImageFont.truetype(str(font),30)
    d.text((80,110),'SCANSIONE SINTETICA',fill='#202020',font=f)
    for i in range(10):
        d.text((80,230+i*85),f'Riga {i+1:02d} - Documento per archivio: 123,45 euro.',fill='#303030',font=f)
    b=io.BytesIO(); img.save(b,format='JPEG',quality=90);b.seek(0)
    c=canvas.Canvas(str(destination/'scansione.pdf'),pagesize=(595,842))
    c.drawImage(ImageReader(b),0,0,595,842)
    c.showPage();c.save()

    base=destination/'documento_ufficio.pdf'
    writer=PdfWriter(clone_from=base)
    writer.encrypt('password-di-test')
    with (destination/'protetto.pdf').open('wb') as stream: writer.write(stream)

    writer=PdfWriter(clone_from=base)
    writer.add_attachment('allegato.txt',b'Allegato sintetico da non perdere.')
    with (destination/'con_allegato.pdf').open('wb') as stream: writer.write(stream)

    # A populated signature field tests conservative detection; this is NOT a cryptographic signature.
    writer=PdfWriter(clone_from=base)
    sig=writer._add_object(DictionaryObject({NameObject('/Type'):NameObject('/Sig'),NameObject('/ByteRange'):ArrayObject([NumberObject(0),NumberObject(1),NumberObject(2),NumberObject(3)]),NameObject('/Contents'):ByteStringObject(b'synthetic-signature')}))
    field=writer._add_object(DictionaryObject({NameObject('/FT'):NameObject('/Sig'),NameObject('/V'):sig,NameObject('/T'):ByteStringObject(b'SignatureTest')}))
    writer._root_object[NameObject('/AcroForm')]=writer._add_object(DictionaryObject({NameObject('/Fields'):ArrayObject([field])}))
    with (destination/'campo_firma_popolato.pdf').open('wb') as stream: writer.write(stream)

    writer=PdfWriter(clone_from=base)
    writer.pages[0].rotate(90)
    with (destination/'pagina_ruotata.pdf').open('wb') as stream: writer.write(stream)

    weird=destination/'società à & (bilancio) 100%.pdf'
    weird.write_bytes(base.read_bytes())
    (destination/'non_valido.pdf').write_bytes(b'%PDF-1.7\nnot a real PDF\n')
    print('Fixtures generated:',destination)


if __name__ == '__main__':
    make(Path(sys.argv[1] if len(sys.argv)>1 else 'test-output/fixtures'))
