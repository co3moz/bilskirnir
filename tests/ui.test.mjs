import { test } from 'node:test';
import assert from 'node:assert/strict';
import { escapeHtml, imageTree, imageChoices, jobLogText, downloadProgress, restartNotice } from '../web/helpers.mjs';
test('completed legacy deletion explains the result rather than waiting or displaying an ID', () => {
  const log = jobLogText({status:'completed',log:'',result:{deleted:'old-environment-id',volumes_preserved:true}});
  assert.equal(log,'Environment deleted. Named volumes were preserved.');
  assert.equal(jobLogText({status:'queued',log:''}),'Waiting to start…');
  assert.equal(jobLogText({status:'failed',log:''}),'Operation failed. No detailed log was recorded.');
  assert.equal(jobLogText({status:'failed',log:'Ownership check failed.'}),'Ownership check failed.');
});
test('deep snapshot history cannot overflow the JavaScript call stack', () => {
  const records = Array.from({length:20000},(_,i)=>({id:`r${i}`,image_id:`image${i}`,parent_image_id:i?`image${i-1}`:null}));
  const tree=imageTree(records); assert.equal(tree.length,20000); assert.equal(tree.at(-1).depth,19999);
});
test('untrusted environment names cannot inject HTML', () => {
  assert.equal(escapeHtml('<img src=x onerror="alert(1)"> &\''), '&lt;img src=x onerror=&quot;alert(1)&quot;&gt; &amp;&#39;');
});
test('image branches follow immutable parent image IDs, not mutable tags', () => {
  const records = [
    { id: 'b', image_id: 'sha:b', parent_image_id: 'sha:a' },
    { id: 'c', image_id: 'sha:c', parent_image_id: 'sha:a' },
    { id: 'a', image_id: 'sha:a', parent_image_id: 'sha:base' },
  ];
  assert.deepEqual(imageTree(records).map(r => [r.id,r.depth]), [['a',0],['b',1],['c',1]]);
});
test('missing parents, duplicate image IDs and cycles remain visible', () => {
  const records = [{ id: 'a', image_id: 'sha:a', parent_image_id: 'sha:b' }, { id: 'b', image_id: 'sha:b', parent_image_id: 'sha:a' }, { id: 'c', image_id: 'sha:a' }];
  assert.equal(imageTree(records).length, 3);
  assert.equal(new Set(imageTree(records).map(r => r.id)).size, 3);
});
test('profile choices distinguish GNOME, XFCE and unavailable images', () => {
  const profiles = [
    {profile:{reference:'xfce:24.04',name:'Ubuntu · Lightweight XFCE'},available:true},
    {profile:{reference:'gnome:24.04',name:'Ubuntu · GNOME'},available:false,image_id:null},
    {profile:{reference:'other:24.04',name:'Ubuntu · Other'},available:false,image_id:'sha256:wrong',error:'Image labels do not match the profile'},
  ];
  const choices = imageChoices(profiles, [{image_id:'sha256:custom',name:'My game',desktop:{title:'Ubuntu · GNOME'}}], 'xfce:24.04');
  assert.equal(choices.length,4);
  // A profile that is not downloaded stays selectable: creating from it downloads the image first.
  assert.deepEqual([choices[1].disabled, choices[1].label], [false, 'Ubuntu · GNOME · downloads first']);
  assert.deepEqual([choices[2].disabled, choices[2].label], [true, 'Ubuntu · Other']);
  assert.equal(choices[3].label,'My game — Ubuntu · GNOME');
  assert.ok(imageChoices([],[],'wsl-desktop/ubuntu:24.04')[0].label.includes('Lightweight XFCE'));
});
test('download progress reads as bytes once the size is known and never claims a size it lacks', () => {
  assert.equal(downloadProgress(null), null);
  assert.deepEqual(downloadProgress({current:0,total:0,layers:0,layers_done:0}), {text:'Preparing download…'});
  // Until every remaining layer reports its size there is no honest bar, only the count so far.
  assert.deepEqual(downloadProgress({current:3 * 1024 ** 2,total:null,layers:4,layers_done:1}), {text:'3.0 MiB downloaded · 1 of 4 layers'});
  assert.deepEqual(downloadProgress({current:0,total:0,layers:2,layers_done:2}), {text:'Checking layers · 2 of 2 layers'});
  const running = downloadProgress({current:1024 ** 3,total:4 * 1024 ** 3,layers:4,layers_done:1});
  assert.deepEqual(running, {value:1024 ** 3,max:4 * 1024 ** 3,percent:25,text:'1.0 GiB of 4.0 GiB · 1 of 4 layers'});
  assert.equal(downloadProgress({current:10,total:10,layers:4,layers_done:3}).text, 'Extracting · 3 of 4 layers');
});
test('the restart warning names the settings that need a new container and what happens', () => {
  assert.equal(restartNotice(['audio'], true), 'Audio needs a new container: the desktop restarts and open applications close. Files in the container are kept.');
  assert.equal(restartNotice(['gpu', 'mounts', 'cpus'], false), 'GPU, volumes and removing the CPU limit need a new container: the desktop stays stopped until you start it. Files in the container are kept.');
});
