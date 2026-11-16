'use strict';

// The pure half of the slash commands (src/commands.js), run by node:
//   node --test tests/
// The completer cases are the TUI's own (Project-JARVIS tests/test_tui_input.py).
const test = require('node:test');
const assert = require('node:assert/strict');
const {
    JARVIS_COMMANDS, commandsFor, isCommandPrefix, matchCommands, filterCommands,
    CommandCompleter, parseCommandLine, parseFlags, takeVisionFlag, hasKeyFlag, maskKeys,
    searchCommands,
} = require('../src/commands.js');

const NAMES = JARVIS_COMMANDS.map((c) => c.name);
const SAMPLE = ['/help', '/export', '/exit', '/status', '/settings'];

test('the catalog is the TUI one, in its order', () => {
    assert.deepEqual(NAMES, [
        '/help', '/?', '/export', '/clear', '/commands', '/quit', '/exit', '/status',
        '/providers', '/model', '/settings', '/setups', '/tasks', '/attach',
        '/new', '/sessions', '/switch', '/rename', '/delete', '/context', '/stop',
    ]);
    for (const c of JARVIS_COMMANDS) {
        assert.ok(['app', 'daemon'].includes(c.where), c.name);
        assert.ok(c.description, c.name);
        assert.equal(typeof c.usage, 'string', c.name);
    }
});

test('a page offers only what its role may use', () => {
    const guest = commandsFor(JARVIS_COMMANDS, () => false).map((c) => c.name);
    assert.ok(guest.includes('/help') && guest.includes('/stop') && guest.includes('/sessions'));
    for (const name of ['/status', '/tasks', '/model', '/new', '/switch', '/attach',
        '/providers', '/settings', '/rename', '/delete', '/quit', '/exit']) {
        assert.ok(!guest.includes(name), name);
    }
    const trusted = commandsFor(JARVIS_COMMANDS, (cls) => cls === 'can-approve').map((c) => c.name);
    assert.ok(trusted.includes('/attach') && trusted.includes('/new'));
    assert.ok(!trusted.includes('/providers') && !trusted.includes('/quit'));
    assert.equal(commandsFor(JARVIS_COMMANDS, () => true).length, JARVIS_COMMANDS.length);
});

test('a bare slash token is a command prefix; a space settles it', () => {
    assert.ok(isCommandPrefix('/'));
    assert.ok(isCommandPrefix('/pro'));
    assert.ok(!isCommandPrefix(''));
    assert.ok(!isCommandPrefix('hello'));
    assert.ok(!isCommandPrefix('what is /pro'));
    assert.ok(!isCommandPrefix('/new title'));
    assert.ok(!isCommandPrefix('/help '));
});

test('Tab completes a unique prefix from the real catalog', () => {
    assert.equal(new CommandCompleter(NAMES).complete('/pro'), '/providers ');
    assert.equal(new CommandCompleter(NAMES).complete('/he'), '/help ');
    assert.equal(new CommandCompleter(NAMES).complete('/PRO'), '/providers ');
});

test('a unique match stays put on repeated Tab', () => {
    const completer = new CommandCompleter(SAMPLE);
    assert.equal(completer.complete('/st'), '/status ');
    assert.equal(completer.complete('/status '), '/status ');
});

test('several matches cycle in catalog order and wrap', () => {
    const completer = new CommandCompleter(SAMPLE);
    assert.equal(completer.complete('/e'), '/export ');
    assert.equal(completer.complete('/export '), '/exit ');
    assert.equal(completer.complete('/exit '), '/export ');
    const real = new CommandCompleter(NAMES);
    assert.equal(real.complete('/e'), '/export ');
    assert.equal(real.complete('/export '), '/exit ');
    assert.equal(real.complete('/exit '), '/export ');
});

test('nothing to complete is null', () => {
    const completer = new CommandCompleter(SAMPLE);
    assert.equal(completer.complete('/zzz'), null);
    assert.equal(completer.complete('hello'), null);
    assert.equal(completer.complete(''), null);
    assert.equal(completer.complete('/new title'), null);
});

test('typing, or a reset, ends the cycle', () => {
    const completer = new CommandCompleter(SAMPLE);
    assert.equal(completer.complete('/e'), '/export ');
    assert.equal(completer.complete('/ex'), '/export ');
    assert.equal(completer.complete('/export '), '/exit ');
    completer.reset();
    assert.equal(completer.complete('/export '), null);
    assert.equal(completer.complete('/export'), '/export ');
});

test('matches and suggestions follow catalog order', () => {
    assert.deepEqual(matchCommands('/e', SAMPLE), ['/export', '/exit']);
    assert.deepEqual(matchCommands('/', SAMPLE), SAMPLE);
    assert.deepEqual(matchCommands('hello', SAMPLE), []);
    assert.deepEqual(filterCommands('/s', JARVIS_COMMANDS).map((c) => c.name),
        ['/status', '/settings', '/setups', '/sessions', '/switch', '/stop']);
    assert.deepEqual(filterCommands('/s ', JARVIS_COMMANDS), []);
});

test('a command line splits into a lower-cased name and its arguments', () => {
    assert.deepEqual(parseCommandLine('  /HELP  foo bar '), { name: '/help', args: 'foo bar' });
    assert.deepEqual(parseCommandLine('/clear'), { name: '/clear', args: '' });
    assert.deepEqual(parseCommandLine('/export\tmy notes'), { name: '/export', args: 'my notes' });
    assert.equal(parseCommandLine('hello /help'), null);
    assert.equal(parseCommandLine(''), null);
});

test('flags read as parse_flags reads them', () => {
    assert.deepEqual(parseFlags(['--type', 'api', '--model', 'm', 'x', '--key']),
        { type: 'api', model: 'm' });
    assert.deepEqual(parseFlags([]), {});
});

test('--vision reads as `jarvis providers add` reads it: bare is on', () => {
    const tokens = (line) => line.split(' ');
    assert.deepEqual(takeVisionFlag(tokens('--type api --model m --vision')),
        { rest: tokens('--type api --model m'), vision: true });
    assert.deepEqual(takeVisionFlag(tokens('--vision --model m')),
        { rest: tokens('--model m'), vision: true });
    assert.deepEqual(takeVisionFlag(tokens('--model m --vision false')),
        { rest: tokens('--model m'), vision: false });
    assert.deepEqual(takeVisionFlag(tokens('--vision TRUE --model m')),
        { rest: tokens('--model m'), vision: true });
    assert.deepEqual(takeVisionFlag(tokens('--vision off')), { rest: [], vision: false });
    assert.deepEqual(takeVisionFlag(tokens('--model m')), { rest: tokens('--model m'), vision: undefined });
});

test('a --key value never shows', () => {
    const line = '/providers add --type api --model m --key sk-secret --url http://x';
    assert.ok(hasKeyFlag(line));
    assert.equal(maskKeys(line), '/providers add --type api --model m --key •••• --url http://x');
    assert.equal(maskKeys('/providers edit a --key  k1 --key k2'), '/providers edit a --key  •••• --key ••••');
    assert.ok(!hasKeyFlag('/providers add --keyboard x'));
    assert.equal(maskKeys('/providers add --keyboard x'), '/providers add --keyboard x');
});

test('help finds the commands that mention every word', () => {
    assert.deepEqual(searchCommands(JARVIS_COMMANDS, 'SESSION prefix').map((c) => c.name),
        ['/switch', '/delete']);
    assert.deepEqual(searchCommands(JARVIS_COMMANDS, 'no such words'), []);
    assert.equal(searchCommands(JARVIS_COMMANDS, '').length, JARVIS_COMMANDS.length);
});
