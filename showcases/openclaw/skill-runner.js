'use strict';

/**
 * OpenClaw skill runner — demonstrates armorSpawn integration.
 *
 * This module is responsible for launching individual OpenClaw skills as
 * subprocesses and wiring their stdio streams to the MCP protocol layer.
 *
 * Before MCP Armor:
 *   Skills are launched with a bare Node.js spawn. Any skill can read
 *   arbitrary files, open network connections, or exec other processes.
 *
 * After MCP Armor (one-line change per spawn site):
 *   Skills are launched through armorSpawn. The armor manifest co-located
 *   with the skill declares exactly what it needs. Everything else is denied
 *   at the kernel level on macOS, and at the broker protocol layer on Linux.
 */

const path = require('path');
const EventEmitter = require('events');

// After: import armorSpawn from the mcparmor package
const { armorSpawn } = require('mcparmor');

// Before: import spawn from Node's child_process
// const { spawn } = require('child_process');

/**
 * @typedef {Object} SkillDescriptor
 * @property {string} name - Unique skill identifier
 * @property {string} command - Executable to run (e.g. "node", "python3")
 * @property {string[]} args - Arguments passed to the command
 * @property {string} armorPath - Path to the skill's armor.json manifest
 * @property {Record<string, string>} [env] - Additional environment variables
 */

/**
 * @typedef {Object} RunningSkill
 * @property {string} name - Skill identifier
 * @property {import('child_process').ChildProcess} process - The subprocess handle
 * @property {number} startedAt - Unix timestamp (ms) when the skill was launched
 */

/**
 * Manages the lifecycle of OpenClaw skill subprocesses.
 *
 * Emits:
 *   - 'skill:ready' (name: string) — skill process started successfully
 *   - 'skill:exit' (name: string, code: number | null) — skill process exited
 *   - 'skill:error' (name: string, err: Error) — skill failed to start
 */
class SkillRunner extends EventEmitter {
  constructor() {
    super();
    /** @type {Map<string, RunningSkill>} */
    this._skills = new Map();
  }

  /**
   * Launches a skill subprocess under MCP Armor enforcement.
   *
   * The skill's armor manifest is read from `skill.armorPath`. If the manifest
   * does not exist or is invalid, armorSpawn rejects before starting the process.
   *
   * @param {SkillDescriptor} skill - The skill to launch
   * @returns {Promise<RunningSkill>} Resolves when the subprocess has started
   * @throws {Error} If the armor manifest is missing or the skill fails to start
   */
  async launch(skill) {
    if (this._skills.has(skill.name)) {
      throw new Error(`Skill '${skill.name}' is already running`);
    }

    const proc = await this._spawnSkill(skill);
    const runningSkill = { name: skill.name, process: proc, startedAt: Date.now() };

    this._skills.set(skill.name, runningSkill);
    this._attachLifecycleHandlers(runningSkill);

    this.emit('skill:ready', skill.name);
    return runningSkill;
  }

  /**
   * Stops a running skill by sending SIGTERM and waiting for exit.
   *
   * @param {string} name - Skill identifier
   * @returns {Promise<void>}
   */
  async stop(name) {
    const skill = this._skills.get(name);
    if (!skill) return;

    skill.process.kill('SIGTERM');
    await new Promise((resolve) => skill.process.once('exit', resolve));
    this._skills.delete(name);
  }

  /**
   * Returns all currently running skill names.
   *
   * @returns {string[]}
   */
  runningSkills() {
    return [...this._skills.keys()];
  }

  /**
   * Spawns the skill subprocess.
   *
   * This is the single site where armorSpawn replaces a bare spawn call.
   * Before MCP Armor, this was:
   *
   *   const proc = spawn(skill.command, skill.args, {
   *     stdio: 'pipe',
   *     env: { ...process.env, ...skill.env },
   *   });
   *
   * After MCP Armor (one-line change):
   *
   *   const proc = armorSpawn(skill.command, skill.args, {
   *     armor: skill.armorPath,
   *     stdio: 'pipe',
   *     env: { ...process.env, ...skill.env },
   *   });
   *
   * The armor manifest at skill.armorPath declares exactly what the skill is
   * allowed to do. Everything else is denied at the enforcement layer.
   *
   * @param {SkillDescriptor} skill
   * @returns {Promise<import('child_process').ChildProcess>}
   */
  async _spawnSkill(skill) {
    // After (one-line change):
    const proc = await armorSpawn(skill.command, skill.args, {
      armor: skill.armorPath,
      stdio: 'pipe',
      env: { ...process.env, ...skill.env },
    });

    // Before:
    // const proc = spawn(skill.command, skill.args, {
    //   stdio: 'pipe',
    //   env: { ...process.env, ...skill.env },
    // });

    return proc;
  }

  /**
   * Wires up exit and error handlers for a running skill.
   *
   * @param {RunningSkill} runningSkill
   */
  _attachLifecycleHandlers(runningSkill) {
    runningSkill.process.on('exit', (code) => {
      this._skills.delete(runningSkill.name);
      this.emit('skill:exit', runningSkill.name, code);
    });

    runningSkill.process.on('error', (err) => {
      this._skills.delete(runningSkill.name);
      this.emit('skill:error', runningSkill.name, err);
    });
  }
}

module.exports = { SkillRunner };
