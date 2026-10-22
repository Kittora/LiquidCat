// Extrae el diccionario I18N del script.js original a JSON (tools/i18n.json).
// Uso: node tools/extract_i18n.js <ruta/script.js>
const fs = require('fs');
const src = fs.readFileSync(process.argv[2], 'utf8');
const a = src.indexOf('const I18N = {');
const b = src.indexOf('function _(key, vars)');
const I18N = new Function(src.slice(a, b) + '; return I18N;')();
fs.writeFileSync(__dirname + '/i18n.json', JSON.stringify(I18N, null, 1));
for (const k of Object.keys(I18N)) console.log(k, Object.keys(I18N[k]).length);
