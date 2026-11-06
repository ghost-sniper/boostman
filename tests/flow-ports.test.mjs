import {readFileSync} from "node:fs";
import {test} from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const spec=JSON.parse(readFileSync(new URL("../src/flowPorts.json",import.meta.url),"utf8"));
const source=readFileSync(new URL("../src/flowPorts.ts",import.meta.url),"utf8").replace('import spec from "./flowPorts.json";',`const spec=${JSON.stringify(spec)};`);
const compiled=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText;
const {ports,canConnect}=await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
test("ports match runtime schema and dynamic fields retain identities",()=>{
 assert.deepEqual(ports({kind:"or"},"inputs").map(p=>p.id),["a","b"]);
 assert.deepEqual(ports({kind:"validate"},"outputs").map(p=>p.id),["pass","fail"]);
 assert.deepEqual(ports({kind:"condition",conditions:[{id:"stable",name:"match"}]},"outputs").map(p=>p.id),["case-stable","default"]);
 assert.deepEqual(ports({kind:"record",entries:[{id:"field",key:"name"}]},"inputs").map(p=>p.id),["item:field"]);
 assert.deepEqual(ports({kind:"evaluate",inputBindings:[{id:"x",name:"price"}]},"inputs").map(p=>p.id),["in","var:x"]);
});
test("connection checks permit fan-out and OR joins but reject occupied ports and cycles",()=>{
 const nodes=[{id:"a",data:{kind:"evaluate"}},{id:"b",data:{kind:"evaluate"}},{id:"or",data:{kind:"or"}}];
 const edge={source:"a",sourceHandle:"out",target:"b",targetHandle:"in"};
 assert.ok(canConnect(nodes,[],edge));assert.ok(canConnect(nodes,[edge],{source:"a",sourceHandle:"out",target:"or",targetHandle:"a"}));
 assert.equal(canConnect(nodes,[edge],edge),false);
 assert.equal(canConnect(nodes,[edge],{source:"b",sourceHandle:"out",target:"a",targetHandle:"in"}),false);
 assert.equal(canConnect(nodes,[],{...edge,targetHandle:"missing"}),false);
});
