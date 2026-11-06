import {readFileSync} from "node:fs";
import {test} from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const compiled=ts.transpileModule(readFileSync(new URL("../src/flowCatalog.ts",import.meta.url),"utf8"),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText;
const {blockGroups,branchOptions,branchNodeIds}=await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
test("catalog includes requested blocks and stable condition/validation branches",()=>{
 const kinds=Object.values(blockGroups).flatMap(g=>g.kinds);
 for(const kind of ["condition","validate","evaluate","or","repeat","collect","response","output","string","bool","number","null","now","date","datetime","list","record","createVariable","getVariable"])assert.ok(kinds.includes(kind),kind);
 assert.deepEqual(branchOptions({kind:"validate"}).map(b=>b.id),["pass","fail"]);
 assert.deepEqual(branchOptions({kind:"condition",conditions:[{id:"stable",name:"Name"}]}).map(b=>b.id),["case-stable","default"]);
});
test("removing condition branch removes its nested controllers without touching siblings",()=>{
 const nodes=[{id:"loop",data:{kind:"repeat",scope:"c:case-a"}},{id:"child",data:{kind:"null",scope:"loop:body"}},{id:"keep",data:{kind:"null",scope:"c:case-b"}}];
 assert.deepEqual([...branchNodeIds(nodes,new Set(["c:case-a"]))],["loop","child"]);
});
