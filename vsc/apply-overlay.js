// Merge product-overlay.json into a packaged product.json (branding, extension gallery, data folder).
const fs = require('fs');
const [target, overlay] = process.argv.slice(2);
const base = JSON.parse(fs.readFileSync(target, 'utf8'));
const over = JSON.parse(fs.readFileSync(overlay, 'utf8'));
fs.writeFileSync(target + '.orig', JSON.stringify(base, null, 2));
fs.writeFileSync(target, JSON.stringify({ ...base, ...over }, null, 2));
console.log('product.json updated:', Object.keys(over).join(', '));
