#!/usr/bin/python3

import psutil
import subprocess
import signal
import os
import time

RLS_DIR = os.path.dirname(os.path.realpath(__file__))
TEST_PROJ_DIR = f"{RLS_DIR}/test-proj"

GOOD_TOOLCHAIN = "leaktest-prev" # Rust #757d6cc91a
BAD_TOOLCHAIN = "leaktest" # Rust #b6e8f9dbdc Remove the `alloc_jemalloc` crate
RELEASE = True
RUN_TIMEOUT_SECS = 15

class Massif:
    @staticmethod
    def profile_cmd(toolchain):
        return f"valgrind --tool=massif --massif-out-file={RLS_DIR}/{toolchain}.massif"

    @staticmethod
    def max_heap_bytes(toolchain):
        filepath = f"{RLS_DIR}/{toolchain}.massif"
        with open(filepath) as profile_out:
            for line in profile_out.readlines():
                if line.startswith("mem_heap_B="):
                    # print(f"mem heap line: {line}")
                    mem_heap_val = int(line[len("mem_heap_B="):])

                elif line.strip() == "heap_tree=peak":
                    return mem_heap_val

        raise Exception(f"No max heap entry found in {filepath}")

    @staticmethod
    def finish(proc, toolchain):
        proc.send_signal(signal.SIGINT)

class Heaptrack:
    @staticmethod
    def profile_cmd(toolchain):
        return "heaptrack"

    @staticmethod
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

    print(f"> LD_LIBRARY_PATH={lib_path_env_var}")

    rls_cmd = f"{RLS_DIR}/target/{toolchain}/{'release' if RELEASE else 'debug'}/rls --cli"

    with open(os.devnull, "w") as dev_null:
        proc = Popen(
            f"rustup run {toolchain} {profiler.profile_cmd(toolchain)} {rls_cmd}",
            env=env,
            cwd=TEST_PROJ_DIR,
            close_fds=False,
            # stdout=dev_null,
            # stderr=dev_null,
        )

        time.sleep(RUN_TIMEOUT_SECS)

        profiler.finish(proc, toolchain)

def summary(toolchain, profiler):
    max_heap_mib = profiler.max_heap_bytes(toolchain) / (1024 ** 2)
    print(f"{toolchain} max heap: {max_heap_mib:.1f}MiB")

def kill_rls(toolchain):
    current_proc = psutil.Process()
    rls_procs = [
        p for p
        in current_proc.children(recursive=True)
        if p.name() == f"rls-rustc-{toolchain}"
    ]

    if len(rls_procs) > 1:
        raise Exception(f"Expected one RLS process for {toolchain}; found {len(rls_procs)}")

    for proc in rls_procs:
        proc.kill()

check_call(f"cargo clean --manifest-path={TEST_PROJ_DIR}/Cargo.toml")
# build(GOOD_TOOLCHAIN)
# build(BAD_TOOLCHAIN)
# profile(GOOD_TOOLCHAIN, PROFILER)
# profile(BAD_TOOLCHAIN, PROFILER)
summary(GOOD_TOOLCHAIN, PROFILER)
summary(BAD_TOOLCHAIN, PROFILER)