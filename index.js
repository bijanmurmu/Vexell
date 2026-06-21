#!/usr/bin/env node

const { program } = require('commander');
const sharp = require('sharp');
const fs = require('fs');

program
  .name('vexell')
  .description('Vexell: Blazing fast lossless SVG to PNG converter')
  .version('1.0.0')
  .argument('<input>', 'Input SVG file')
  .argument('<output>', 'Output PNG file')
  .option('-s, --size <pixels>', 'Output resolution (width and height)', 1024)
  .action((input, output, options) => {
    if (!fs.existsSync(input)) {
      console.error(`Error: File ${input} not found.`);
      process.exit(1);
    }

    const size = parseInt(options.size);

    console.log(`Vexellizing ${input} -> ${output}...`);

    sharp(input)
      .resize(size, size, { fit: 'inside' })
      .png({ compressionLevel: 9, adaptiveFiltering: true, force: true })
      .toFile(output)
      .then(info => {
        console.log(`Successfully rendered! (${info.width}x${info.height})`);
      })
      .catch(err => {
        console.error(`Error converting file: ${err.message}`);
        process.exit(1);
      });
  });

program.parse();
