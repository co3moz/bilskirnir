import { test } from 'node:test';
import assert from 'node:assert/strict';
import { escapeHtml, imageTree, imageChoices, jobLogText } from '../web/helpers.mjs';
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
    {profile:{reference:'gnome:24.04',name:'Ubuntu · GNOME'},available:false},
  ];
  const choices = imageChoices(profiles, [{image_id:'sha256:custom',name:'My game',desktop:{title:'Ubuntu · GNOME'}}], 'xfce:24.04');
  assert.equal(choices.length,3);
  assert.equal(choices[1].disabled,true);
  assert.equal(choices[2].label,'My game — Ubuntu · GNOME');
  assert.ok(imageChoices([],[],'wsl-desktop/ubuntu:24.04')[0].label.includes('Lightweight XFCE'));
});

