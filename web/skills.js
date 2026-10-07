let skillCatalog=null;
async function loadSkillCatalog(){
  if(!settings)await loadSettings();
  if(!skillCatalog)skillCatalog=await api('/api/skills');
  if(!Array.isArray(skillCatalog.skills)){skillCatalog=null;throw new Error('Skillkatalog fehlt');}
  renderSkillCatalog();
}
function renderSkillCatalog(){
  if(!skillCatalog)return;
  const q=$('#catalogSearch').value.trim().toLocaleLowerCase(),cls=$('#catalogClass').value,variants=$('#catalogVariants').checked;
  const rows=[...skillCatalog.skills,...(variants?skillCatalog.variants||[]:[])].filter(s=>(!cls||s.class_key===cls)&&(!q||[s.id,...s.aliases||[],...Object.values(s.names||{})].join(' ').toLocaleLowerCase().includes(q))).sort((a,b)=>skillName(a).localeCompare(skillName(b),skillLanguage())||a.id-b.id);
  const offset=Math.max(0,Math.min(catalogPage*80,Math.max(0,Math.ceil(rows.length/80)-1)*80));catalogPage=offset/80;
  $('#catalogCount').textContent=`${rows.length} Treffer · ${skillCatalog.skills.length} Hauptfähigkeiten · ${skillCatalog.translation_note}`;
  $('#catalogRows').innerHTML=rows.slice(offset,offset+80).map(s=>`<tr><td>${skillLabel(s)}${s.name_source_de==='community'?'<small class="muted"> · DE Community</small>':''}</td><td>${esc(s.names?.de||'—')}</td><td>${esc(s.names?.en||'—')}</td><td>${classIcon(s)}${esc(localizedClass(s)||'Allgemein / Variante')}</td><td class="num">${s.id}${s.aliases?.length?`<details><summary>${s.aliases.length} Varianten</summary>${s.aliases.join(', ')}</details>`:''}</td></tr>`).join('');
  $('#catalogPrev').disabled=offset===0;$('#catalogNext').disabled=offset+80>=rows.length;$('#catalogPage').textContent=`${rows.length?offset+1:0}–${Math.min(rows.length,offset+80)}`;
}
let catalogPage=0;
for(const id of ['catalogSearch','catalogClass','catalogVariants'])$('#'+id).addEventListener(id==='catalogSearch'?'input':'change',()=>{catalogPage=0;renderSkillCatalog();});
$('#catalogPrev').onclick=()=>{catalogPage--;renderSkillCatalog();};$('#catalogNext').onclick=()=>{catalogPage++;renderSkillCatalog();};
if(tab==='skills')task(loadSkillCatalog());
