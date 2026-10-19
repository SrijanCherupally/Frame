"""Own child processes so cancellation and app shutdown never orphan FFmpeg."""

import subprocess
import threading


class Cancelled(RuntimeError):
    pass


class ProcessRegistry:
    def __init__(self):
        self.lock = threading.RLock()
        self.children = {}
        self.cancelled = set()
        self.stopping = False

    def start(self, command, group, **kwargs):
        with self.lock:
            if self.stopping or group in self.cancelled:
                raise Cancelled("Operation cancelled")
            proc = subprocess.Popen(command, **kwargs)
            self.children[proc] = group
            return proc

    def release(self, proc):
        with self.lock:
            self.children.pop(proc, None)

    def run(self, command, group="media", timeout=None):
        proc = self.start(
            command, group, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        try:
            out, err = proc.communicate(timeout=timeout)
            if group in self.cancelled or self.stopping:
                raise Cancelled("Operation cancelled")
            if proc.returncode:
                raise subprocess.CalledProcessError(proc.returncode, command, out, err)
            return out
        except BaseException:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            raise
        finally:
            self.release(proc)

    def cancel(self, group):
        with self.lock:
            self.cancelled.add(group)
            for proc, g in list(self.children.items()):
                if g == group and proc.poll() is None:
                    proc.terminate()

    def stop_all(self):
        with self.lock:
            self.stopping = True
            processes = list(self.children)
            for proc in processes:
                if proc.poll() is None:
                    proc.terminate()
        for proc in processes:
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


PROCESSES = ProcessRegistry()
