let skillCatalog=null,catalogLoad=null,catalogPage=0;
const catalogStorageKey='a2m-catalog-view-v1';
// Only display filters are stored here. Storage can be unavailable in OBS/private mode.
function restoreCatalogView(){
  try {
    const saved=JSON.parse(localStorage.getItem(catalogStorageKey));
    if(!saved||typeof saved!=='object')return;
    $('#catalogSearch').value=typeof saved.query==='string'?saved.query.slice(0,200):'';
    $('#catalogClass').value=classKeys.includes(saved.class_key)?saved.class_key:'';
    $('#catalogVariants').checked=saved.variants===true;
    catalogPage=Number.isInteger(saved.page)&&saved.page>=0&&saved.page<=10000?saved.page:0;
  } catch(e) {}
}
function saveCatalogView(){
  try {localStorage.setItem(catalogStorageKey,JSON.stringify({query:$('#catalogSearch').value,class_key:$('#catalogClass').value,variants:$('#catalogVariants').checked,page:catalogPage}));}catch(e) {}
}
async function loadSkillCatalog(){
  if(!settings)await loadSettings();
  if(!skillCatalog){
    if(!catalogLoad)catalogLoad=api('/api/skills').then(data=>{
      if(!Array.isArray(data?.skills))throw new Error('Skillkatalog fehlt');
      skillCatalog=data;
    }).finally(()=>{catalogLoad=null;});
    await catalogLoad;
  }
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
  $('#catalogEmpty').hidden=rows.length!==0;
  $('#catalogReset').disabled=!$('#catalogSearch').value&&!cls&&!variants&&catalogPage===0;
  saveCatalogView();
}
restoreCatalogView();
for(const id of ['catalogSearch','catalogClass','catalogVariants'])$('#'+id).addEventListener(id==='catalogSearch'?'input':'change',()=>{catalogPage=0;renderSkillCatalog();});
$('#catalogReset').onclick=()=>{$('#catalogSearch').value='';$('#catalogClass').value='';$('#catalogVariants').checked=false;catalogPage=0;renderSkillCatalog();$('#catalogSearch').focus();};
$('#catalogPrev').onclick=()=>{catalogPage--;renderSkillCatalog();};$('#catalogNext').onclick=()=>{catalogPage++;renderSkillCatalog();};
if(tab==='skills')task(loadSkillCatalog());
