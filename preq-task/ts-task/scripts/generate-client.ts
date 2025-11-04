import { createFromRoot } from 'codama';
import { rootNodeFromAnchor, type AnchorIdl } from '@codama/nodes-from-anchor';
import { renderVisitor as renderJavaScriptVisitor } from "@codama/renderers-js";
import anchorIdl from '../programs/Turbin3_prereq.json';
import path from 'path';
import { fileURLToPath } from 'url';

// Get the directory name for ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const codama = createFromRoot(rootNodeFromAnchor(anchorIdl as AnchorIdl));
const jsClient = path.join(__dirname, "..", "clients", "js");
codama.accept(renderJavaScriptVisitor(path.join(jsClient, "src", "generated")));
