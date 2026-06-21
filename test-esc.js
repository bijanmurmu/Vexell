const readline = require('readline');

readline.emitKeypressEvents(process.stdin);
process.stdin.on('keypress', (str, key) => {
  if (key && key.name === 'escape') {
    console.log('\nAborted by ESC.');
    process.exit(0);
  }
});

async function main() {
  const { input } = await import('@inquirer/prompts');
  await input({ message: 'Type something:' });
  console.log('Done');
  process.exit(0);
}
main();
