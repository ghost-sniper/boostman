import { readFileSync } from "node:fs";
import { existsSync } from "node:fs";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:http";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { test } from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const compiled = ts.transpileModule(readFileSync(new URL("../src/httpTransfer.ts", import.meta.url), "utf8"), {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText;
const {importCurl, exportHttp, curlTokens} = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const req = (extra = {}) => ({id:"r",kind:"http",name:"示例",method:"POST",url:"http://localhost/test",headers:[],query:[],body:{type:"none"},auth:{type:"none"},timeoutMs:30000,followRedirects:true,...extra});

test("imports multiline JSON, duplicate headers, cookie and method", () => {
  const {config} = importCurl(`curl 'http://localhost/api?a=1&a=2' \\\n -X PATCH -H 'Content-Type: application/json' -H 'X-Test: a' -H 'X-Test: b' -b 'sid=abc' --data-raw '{"title":"O'\\''Brien","value":"$HOME"}' -L -m 2.5`);
  assert.equal(config.method,"PATCH"); assert.equal(config.timeoutMs,2500); assert.equal(config.followRedirects,true);
  assert.equal(config.body.type,"json"); assert.equal(JSON.parse(config.body.text).title,"O'Brien");
  assert.deepEqual(config.headers.filter(h=>h.key==="X-Test").map(h=>h.value),["a","b"]);
  assert.equal(config.url,"http://localhost/api?a=1&a=2");
});
test("auth secrets are returned separately for system credential storage", () => {
  const a=importCurl("curl http://localhost -u 'user:pa:ss'");
  assert.deepEqual(a.config.auth,{type:"basic",username:"user"}); assert.equal(a.secret,"pa:ss");
  const b=importCurl("curl http://localhost -H 'Authorization: Bearer token123'");
  assert.equal(b.secret,"token123"); assert.equal(b.config.headers.length,0);
  assert.equal(JSON.stringify(b.config).includes("token123"),false);
});
test("imports form, GET data, raw @ and file bodies", () => {
  assert.equal(importCurl("curl http://localhost --data-urlencode 'name=a+b = x'").config.body.fields[0].value,"a+b = x");
  const g=importCurl("curl http://localhost -G -d 'q=a+b&x=%26'").config;
  assert.equal(g.method,"GET"); assert.deepEqual(g.query.map(p=>p.value),["a b","&"]);
  assert.equal(importCurl("curl http://localhost --data-raw '@literal'").config.body.text,"@literal");
  assert.equal(importCurl("curl http://localhost --data-binary '@C:\\data\\file.bin'").config.body.path,"C:\\data\\file.bin");
  assert.equal(importCurl("curl http://localhost --head").config.method,"HEAD");
});
test("rejects unsupported or ambiguous commands instead of silently dropping flags", () => {
  for (const source of ["curl http://localhost --insecure", "curl http://localhost http://other", "curl 'unterminated", "curl http://localhost | sh", "curl http://localhost; echo bad", "curl http://localhost --data @file", "curl http://localhost -d x -F a=b", "curl http://localhost --max-time nope", "curl http://localhost --data $'a\\nb'", "curl ftp://localhost"]) assert.throws(()=>importCurl(source),undefined,source);
});
test("POSIX quoting preserves newlines, apostrophes, backslashes and shell metacharacters", () => {
  const text = `a' b " $HOME $(echo bad) \`literal\` \\ \n中文`;
  const r=req({body:{type:"text",text},headers:[{key:"X-A",value:"a'b",enabled:true},{key:"X-Off",value:"ignored",enabled:false}],query:[{key:"q",value:"a & b",enabled:true}]});
  const script=exportHttp(r,"curl"), imported=importCurl(script).config;
  assert.equal(imported.body.text,text); assert.equal(imported.headers.some(h=>h.key==="X-Off"),false);
  assert.equal(new URL(imported.url).searchParams.get("q"),"a & b");
  assert.ok(exportHttp(r,"bash").startsWith("#!/usr/bin/env bash\n"));
  const ps=exportHttp(r,"powershell"); assert.match(ps,/Invoke-WebRequest/); assert.equal(ps.includes("curl.exe"),false); assert.ok(ps.includes("a''b"));
});
test("multipart and form exports preserve text/file distinction and disabled fields", () => {
  const fields=[{key:"file",value:"C:\\data\\a b.txt",type:"file"},{key:"literal",value:"@data;type=text",type:"text"},{key:"skip",value:"no",enabled:false}];
  const r=req({body:{type:"multipart",fields}});
  assert.deepEqual(importCurl(exportHttp(r,"curl")).config.body.fields.map(f=>[f.key,f.value,f.type]),fields.slice(0,2).map(f=>[f.key,f.value,f.type]));
  const form=req({body:{type:"form",fields:[{key:"a b",value:"a & b"}]}});
  assert.equal(importCurl(exportHttp(form,"curl")).config.body.fields[0].key,"a b");
  assert.match(exportHttp(req({method:"HEAD"}),"curl"),/--head/);
});
test("export credentials are placeholders and templates are retained in URL/body", () => {
  const r=req({url:"https://example.com/{{path}}",auth:{type:"basic",username:"me",secretId:"id-123"},body:{type:"text",text:"{{payload}}"}});
  for(const f of ["curl","powershell","bash"]) {const s=exportHttp(r,f); assert.ok(s.includes("{{secret:id-123}}"));assert.ok(s.includes("{{path}}"));assert.ok(s.includes("{{payload}}"));}
});
test("Bash uses GNU Wget and preserves method, headers and timeout settings", () => {
  const script=exportHttp(req({method:"PATCH",followRedirects:false,timeoutMs:2500,body:{type:"form",fields:[{key:"x",value:"a & b"},{key:"x",value:"second"}]}}),"bash");
  assert.match(script,/wget \\/); assert.equal(script.includes("curl"),false);
  for(const flag of ["--method=PATCH","--timeout=2.5","--max-redirect=0","--tries=1","--body-data=x=a+%26+b&x=second"]) assert.ok(script.includes(flag),flag);
});

test("Bash quoting and multipart bytes survive execution with a capture Wget", async t => {
  const bash=process.platform==="win32" ? join(process.env.ProgramFiles || "C:/Program Files","Git/bin/bash.exe") : "bash";
  if(process.platform==="win32" && !existsSync(bash)) return t.skip("Git Bash not installed");
  const directory=await mkdtemp(join(tmpdir(),"boostman-wget-"));
  const run=promisify(execFile);
  try {
    const helper=join(directory,"capture.cjs"), script=join(directory,"request.sh"), file=join(directory,"a b.bin").replaceAll("\\","/");
    await writeFile(file,Buffer.from([0,1,13,10,255]));
    await writeFile(helper,`const fs=require('fs');const args=process.argv.slice(2);const file=args.find(a=>a.startsWith('--body-file='));console.log(JSON.stringify({args,body:file?fs.readFileSync(file.slice(12)).toString('base64'):null}));`);
    const execute=async r=>{
      await writeFile(script,exportHttp(r,"bash"));
      const result=await run(bash,["-c",'wget(){ node "$CAPTURE_HELPER" "$@"; }; source "$1"',"boostman",script.replaceAll("\\","/")],{env:{...process.env,CAPTURE_HELPER:helper.replaceAll("\\","/")},timeout:15000});
      return JSON.parse(result.stdout);
    };
    const text=`中文 O'Brien $HOME $(literal) \`literal\`\nend`;
    const raw=await execute(req({body:{type:"json",text},auth:{type:"basic",username:"me",secretId:"id"}}));
    assert.ok(raw.args.includes(`--body-data=${text}`));
    assert.ok(raw.args.includes(`--header=Authorization: Basic ${Buffer.from("me:{{secret:id}}").toString("base64")}`));
    const multi=await execute(req({body:{type:"multipart",fields:[{key:"text",value:text},{key:"file",value:file,type:"file"},{key:"text",value:"duplicate"}]}}));
    const body=Buffer.from(multi.body,"base64");
    assert.ok(body.includes(Buffer.from(text))); assert.ok(body.includes(Buffer.from([0,1,13,10,255])));
    assert.ok(body.includes(Buffer.from('filename="a b.bin"'))); assert.ok(body.includes(Buffer.from("duplicate")));
  } finally { await rm(directory,{recursive:true,force:true}); }
});

test("Invoke-WebRequest sends forms, binary files and multipart without curl", {skip:process.platform!=="win32"}, async () => {
  const received=[];
  const server=createServer(async (request,response)=>{const chunks=[];for await(const c of request)chunks.push(c);received.push({headers:request.headers,body:Buffer.concat(chunks)});response.statusCode=422;response.setHeader("Content-Type","application/json");response.end('{"accepted":true}');});
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const directory=await mkdtemp(join(tmpdir(),"boostman-iwr-"));
  const run=promisify(execFile);
  try {
    const path=join(directory,"request.ps1"),file=join(directory,"a b.bin"),bytes=Buffer.from([0,1,13,10,255]);
    await writeFile(file,bytes);
    const execute=async extra=>{
      const code=exportHttp(req({url:`http://127.0.0.1:${server.address().port}/echo`,...extra}),"powershell");
      assert.equal(code.includes("curl"),false); await writeFile(path,code);
      const output=await run("pwsh",["-NoProfile","-NonInteractive","-File",path],{timeout:15000});
      assert.deepEqual(JSON.parse(output.stdout),{accepted:true}); return received.at(-1);
    };
    const form=await execute({body:{type:"form",fields:[{key:"x",value:"a & b"},{key:"x",value:"中文"}]}});
    assert.equal(form.body.toString(),"x=a+%26+b&x=%E4%B8%AD%E6%96%87");
    const binary=await execute({body:{type:"file",path:file}}); assert.deepEqual(binary.body,bytes);
    const multi=await execute({body:{type:"multipart",fields:[{key:"text",value:"中文 O'Brien"},{key:"file",value:file,type:"file"},{key:"text",value:"duplicate"}]},auth:{type:"basic",username:"me",secretId:"id"}});
    assert.ok(multi.body.includes(bytes)); assert.ok(multi.body.includes(Buffer.from("中文 O'Brien")));assert.ok(multi.body.includes(Buffer.from("duplicate")));
    assert.equal(multi.headers.authorization,`Basic ${Buffer.from("me:{{secret:id}}").toString("base64")}`);
  } finally { server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await rm(directory,{recursive:true,force:true}); }
});
test("exported curl and native scripts send exact JSON to a local echo server", async () => {
  const server=createServer(async (request,response)=>{const chunks=[];for await(const c of request)chunks.push(c);response.setHeader("Content-Type","application/json");response.end(JSON.stringify({method:request.method,url:request.url,body:Buffer.concat(chunks).toString(),header:request.headers["x-test"]}));});
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const directory=await mkdtemp(join(tmpdir(),"boostman-transfer-"));
  const run=promisify(execFile);
  try {
    const text=JSON.stringify({value:`中文 O'Brien "double" $HOME $(literal) \\ newline\nnext`});
    const r=req({url:`http://127.0.0.1:${server.address().port}/echo`,headers:[{key:"X-Test",value:"a'b"}],body:{type:"json",text},query:[{key:"q",value:"a & b"}]});
    const expected={method:"POST",url:"/echo?q=a%20%26%20b",body:text,header:"a'b"};
    const args=curlTokens(exportHttp(r,"curl")).slice(1);
    const curl=await run(process.platform==="win32" ? "curl.exe" : "curl",args,{timeout:15000});
    assert.deepEqual(JSON.parse(curl.stdout),expected);
    const format=process.platform==="win32" ? "powershell" : "bash";
    const path=join(directory,format==="powershell"?"request.ps1":"request.sh");
    await writeFile(path,exportHttp(r,format));
    const script=await run(format==="powershell"?"pwsh":"bash",format==="powershell"?["-NoProfile","-NonInteractive","-File",path]:[path],{timeout:20000});
    assert.deepEqual(JSON.parse(script.stdout),expected);
  } finally { server.closeAllConnections(); await new Promise(resolve=>server.close(resolve)); await rm(directory,{recursive:true,force:true}); }
});

