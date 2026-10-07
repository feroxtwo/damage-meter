#!/usr/bin/env python3
"""Rebuild offline assets from observed source manifests; never downloads data.
Requires Pillow only for this maintenance step (not building/running the meter).
Usage: python scripts/import-skill-catalog.py --sources DIR --wakayashi FILE
DIR contains meter-quest-{skills,en-final,extra-final,downloads}.json and
meter-wakayashi-downloads-final.json; downloaded paths are resolved under DIR.
"""
import argparse, collections, hashlib, io, json, pathlib
from urllib.parse import urlsplit
from PIL import Image
ROOT = pathlib.Path(__file__).resolve().parents[1]
CLASSES = ['gladiator','templar','assassin','ranger','sorcerer','elementalist','cleric','chanter','fighter']
# Questlog DE has no fighter translations yet. Explicit, editable community names.
FIGHTER_DE = dict(zip([19010000,19040000,19070000,19090000,19130000,19160000,19170000,19200000,19230000,19250000,19280000,19290000,19300000,19310000,19320000,19330000,19350000,19360000,19370000,19380000,19390000,19400000,19420000,19450000,19460000,19710000,19720000,19730000,19740000,19750000,19760000,19770000,19780000,19790000,19800000,19020000,19030000,19050000,19060000,19270000,19410000],
['Schlaghagel','Rundumschlag','Wirbelwind','Bodenerschütterung','Amoklauf','Aufwärtsschlag','Fegender Tritt','Explodierende Faust','Schulterstoß','Taifun-Schlaghagel','Wildheit','Wogender Hieb','Beben','Zermalmen','Blutstrom unterbrechen','Phantom','Konzentrierte Abwehr','Unaufhaltsam','Volle Kraft','Sturmangriff','Energie entladen','Zeitbombe','Kettenfaust','Trotz','Blitzschritt','Stellung halten','Frontlinie durchbrechen','Geistige Umwandlung','Dominanz','Verstärkter Amoklauf','Schockschlag','Gewaltiger Zorn','Wuchtschlag','Pakt der Wiederbelebung','Leichtgewicht','Sturzflut','Prügel','Kreuzhaken','Schwinger','Orkantritt','Zurückspulen']))

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--sources',type=pathlib.Path,required=True);ap.add_argument('--wakayashi',type=pathlib.Path,required=True);args=ap.parse_args();base=args.sources
    read=lambda n:json.loads((base/n).read_text())
    de={s['id']:s for s in read('meter-quest-skills.json')};en={s['id']:s for s in read('meter-quest-en-final.json')};extra={s['id']:s for s in read('meter-quest-extra-final.json')}
    downloads={d['src']:base/pathlib.Path(urlsplit(d['src']).path).name for n in ['meter-quest-downloads.json','meter-wakayashi-downloads-final.json'] for d in read(n)}
    waka={}
    for c in json.loads(args.wakayashi.read_text()):
        def walk(v):
            if isinstance(v,dict):
                if v.get('nameEn') and v.get('iconSrc'):waka[(c['nameEn'].lower(),v['nameEn'])]=v
                for x in v.values():walk(x)
            elif isinstance(v,list):
                for x in v:walk(x)
        walk(c['skills']);walk(c['stigmaSwaps'])
    def waka_url(src):
        candidates=[url for url in downloads if src.split('?')[0] in url]
        return candidates[0] if len(candidates)==1 else None
    assets={};pack=bytearray();dedup={};rgba=bytearray()
    def asset(name,url):
        if not url or not downloads[url].exists():return None
        image=Image.open(downloads[url]).convert('RGBA');image.thumbnail((64,64),Image.Resampling.LANCZOS);out=io.BytesIO();image.save(out,format='WEBP',quality=70,method=6);data=out.getvalue();sha=hashlib.sha256(data).hexdigest()
        if sha not in dedup:dedup[sha]=(len(pack),len(data));pack.extend(data)
        offset,length=dedup[sha];assets[name]={'offset':offset,'length':length,'sha256':sha,'source':url};return '/assets/icons/'+name
    entries=[]
    for id,s in sorted(en.items()):
        ds=de.get(id,{});src=ds.get('source_icon') or extra.get(id,{}).get('source_icon');cls=CLASSES[id//1000000-11] if 11<=id//1000000<=19 else 'unknown'
        names={'de':ds.get('name_de') or FIGHTER_DE.get(id) or s['name_en'],'en':s['name_en']};w=waka.get((cls,s['name_en']));ws=waka_url(w['iconSrc']) if w else None
        if ws and not downloads[ws].exists():ws=None
        entry={'id':id,'names':names,'class_key':cls,'icon':asset(f'skill-{id}.webp',ws or src),'name_source_de':'community' if id in FIGHTER_DE else 'questlog','sources':{'questlog':src,'wakayashi':ws},'aliases':[]}
        entries.append(entry)
    # Only exact English identities are mapped. Family ID alone never assigns art.
    original=json.loads((ROOT/'data/i18n/skills/en.json').read_text());by_name=collections.defaultdict(list);by_id={s['id']:s for s in entries}
    for s in entries:by_name[s['names']['en']].append(s)
    for raw,name in original.items():
        id=int(raw)
        if id in by_id:continue
        candidates=by_name.get(name,[]);family=by_id.get(id//10000*10000)
        target=family if family in candidates else candidates[0] if len(candidates)==1 else None
        if target:target['aliases'].append(id)
    for cls in CLASSES:
        url=next(u for u in downloads if u.endswith('/UT_Class_'+cls.title()+'_Large.webp'))
        if not downloads[url].exists():
            url=next(u for u in downloads if '/aion2-emblem-'+cls+'.webp' in u and downloads[u].exists())
        asset(f'class-{cls}.webp',url);rgba.extend(Image.open(downloads[url]).convert('RGBA').resize((32,32),Image.Resampling.LANCZOS).tobytes())
    out=ROOT/'data/skills';out.mkdir(exist_ok=True)
    catalog={'revision':'2026-10-07','skills':entries,'classes':CLASSES,'translation_note':'Faustkämpfer: deutsche Community-Übersetzungen; Questlog DE enthält diese Klasse noch nicht.'}
    for name,obj in [('catalog.json',catalog),('icons.json',assets)]: (out/name).write_text(json.dumps(obj,ensure_ascii=False,indent=2)+'\n')
    (out/'icons.bin').write_bytes(pack);(out/'classes.rgba').write_bytes(rgba)
    print(f'{len(entries)} primary skills; {sum(len(s["aliases"]) for s in entries)} exact aliases; {sum(bool(s["icon"]) for s in entries)} skill icons; {len(pack)} packed bytes')
if __name__=='__main__':main()
