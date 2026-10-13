import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FileUploads } from '../web/uploads.mjs';

function fixture() {
  const requests = [];
  let unauthorized = 0;
  const uploads = new FileUploads({
    createRequest: () => {
      const request = {upload:{},headers:{},open(method,url){this.url=url;},setRequestHeader(key,value){this.headers[key]=value;},send(file){this.file=file;},abort(){this.onabort();},complete(status,result){this.status=status;this.responseText=JSON.stringify(result);this.onload();}};
      requests.push(request); return request;
    },
    onUnauthorized: () => unauthorized++,
  });
  return {uploads,requests,get unauthorized(){return unauthorized;}};
}
const options = {environmentId:'environment',destination:'/shared/project',replace:false};
const tick = () => new Promise(resolve=>setImmediate(resolve));

test('files upload sequentially with encoded names, CSRF and progress', async () => {
  const {uploads,requests} = fixture();
  uploads.select([{name:'file & #.txt',size:10},{name:'empty.txt',size:0}],100);
  const done = uploads.start(options);
  assert.equal(requests.length,1);
  assert.equal(new URL(requests[0].url,'http://localhost').searchParams.get('name'),'file & #.txt');
  assert.equal(requests[0].headers['X-Wsld-Request'],'1');
  requests[0].upload.onprogress({loaded:5}); assert.equal(uploads.items[0].loaded,5);
  requests[0].complete(200,{path:'/shared/project/file & #.txt'}); await tick();
  assert.equal(requests.length,2);
  requests[1].complete(200,{path:'/shared/project/empty.txt'}); await done;
  assert.equal(uploads.running,false);
  assert.ok(uploads.items.every(item=>item.status==='completed'));
});
test('cancellation aborts the active file and does not send the remaining files', async () => {
  const {uploads,requests} = fixture();
  uploads.select([{name:'first',size:5},{name:'second',size:5}],100);
  const done = uploads.start(options); uploads.cancel(); await done;
  assert.equal(requests.length,1); assert.equal(uploads.running,false);
  assert.ok(uploads.items.every(item=>item.status==='cancelled'));
});
test('session expiry stops the queue and oversized selection sends no requests', async () => {
  const f = fixture();
  assert.throws(()=>f.uploads.select([{name:'big',size:101}],100),/smaller/);
  f.uploads.select([{name:'first',size:5},{name:'second',size:5}],100);
  const done = f.uploads.start(options);
  f.requests[0].complete(401,{error:'Sign in required'}); await done;
  assert.equal(f.unauthorized,1); assert.equal(f.requests.length,1);
});
