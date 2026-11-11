// Tab edits source; Escape then Tab preserves a keyboard path out of the editor.
export function sourceEditor(editor){
  let escapeTab=false;
  editor.addEventListener('keydown',event=>{
    if(event.key==='Escape'){escapeTab=true;return}
    if(event.key!=='Tab'){if(!['Shift','Control','Meta','Alt'].includes(event.key))escapeTab=false;return}
    if(escapeTab){escapeTab=false;return}
    event.preventDefault();
    const start=editor.selectionStart,end=editor.selectionEnd,text=editor.value;
    if(start===end&&!event.shiftKey){editor.setRangeText('  ',start,end,'end')}
    else{
      const from=start===0?0:text.lastIndexOf('\n',start-1)+1;
      let to=text.indexOf('\n',end>start&&text[end-1]==='\n'?end-1:end);if(to<0)to=text.length;
      const block=text.slice(from,to),lines=block.split('\n');
      const removed=line=>line.startsWith('\t')?1:Math.min(2,line.match(/^ */)[0].length);
      const delta=event.shiftKey?-removed(lines[0]):2;
      const replacement=lines.map(line=>event.shiftKey?line.slice(removed(line)):'  '+line).join('\n');
      editor.setRangeText(replacement,from,to,'select');
      editor.setSelectionRange(Math.max(from,start+Math.max(delta,from-start)),Math.max(from,end+replacement.length-block.length));
    }
    editor.dispatchEvent(new Event('input',{bubbles:true}));
  });
}
