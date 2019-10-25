#!/usr/bin/python3

import psutil
import subprocess
import signal
import os
import time

RLS_DIR = os.path.dirname(os.path.realpath(__file__))
TEST_PROJ_DIR = f"{RLS_DIR}/test-proj"

OLD_VER = "leaktest-prev"
NEW_VER = "leaktest"
RELEASE = False
RUN_TIMEOUT_SECS = 10

class Massif:
    def profile_cmd(toolchain):
        return f"valgrind --tool=massif --massif-out-file={RLS_DIR}/{toolchain}.massif"

    def finish(proc, toolchain):
        proc.send_signal(signal.SIGINT)

class Heaptrack:
    def profile_cmd(toolchain):
        return "heaptrack"

    def finish(proc, toolchain):
        kill_rls(toolchain)


PROFILER = Massif

def print_and_split(subproc_fn):
    def run(cmd, **kwargs):
        print(f"> {cmd}")
        return subproc_fn(cmd.split(" "), **kwargs)

    return run

check_call = print_and_split(subprocess.check_call)
check_output = print_and_split(subprocess.check_output)
Popen = print_and_split(subprocess.Popen)

def build(toolchain):
    check_call(
        f"rustup run {toolchain} cargo build --target-dir={RLS_DIR}/target/{toolchain} --no-default-features{' --release' if RELEASE else ''}"
    )

def profile(toolchain, profiler):
    toolchain_path = check_output(
        f"rustup run {toolchain} rustc --print sysroot"
    ).decode("ascii").rstrip()
    lib_path_env_var = f"{toolchain_path}/lib"

    env = os.environ.copy()
    env["LD_LIBRARY_PATH"] = lib_path_env_var

    print(f"lib_path_env_var={lib_path_env_var}")

    rls_cmd = f"{RLS_DIR}/target/{toolchain}/{'release' if RELEASE else 'debug'}/rls --cli"

    proc = Popen(
        f"{profiler.profile_cmd(toolchain)} {rls_cmd}",
        env=env,
        cwd=TEST_PROJ_DIR,
        close_fds=False
    )

    time.sleep(RUN_TIMEOUT_SECS)

    profiler.finish(proc, toolchain)

def kill_rls(toolchain):
    current_proc = psutil.Process()
    rls_procs = [
        p for p
        in current_proc.children(recursive=True)
        if p.name() == f"rls-rustc-{toolchain}"
    ]

    if len(rls_procs) != 1:
        raise Exception(f"Expected one RLS process for {toolchain}; found {len(rls_procs)}")

    for proc in rls_procs:
        proc.kill()

build(OLD_VER)
build(NEW_VER)
profile(OLD_VER, PROFILER)
profile(NEW_VER, PROFILER)