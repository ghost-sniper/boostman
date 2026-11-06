import {readFileSync} from "node:fs";
import {test} from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const compiled=ts.transpileModule(readFileSync(new URL("../src/flowPresentation.ts",import.meta.url),"utf8"),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText;
const {flowResults,linePoints}=await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
test("display results restore from final context when events are missing",()=>{
 const last={status:"succeeded",meta:{display:{mode:"boolean",value:false}}};
 const restored=flowResults({events:[{nodeId:"d",result:{meta:{display:{value:true}}}}],result:{context:{nodes:{d:last,failed:{status:"failed"}}}}});
 assert.deepEqual(restored.d,last);assert.equal(restored.failed.status,"failed");assert.deepEqual(flowResults(null),{});
});
test("line charts preserve order and custom axes without coercing string numbers",()=>{
 assert.deepEqual(linePoints([3,1,2]),[{x:"1",y:3},{x:"2",y:1},{x:"3",y:2}]);
 assert.deepEqual(linePoints([{name:"week1",price:2},{name:"week2",price:"3"}],"name","price"),[{x:"week1",y:2}]);
 assert.deepEqual(linePoints([]),[]);
});
