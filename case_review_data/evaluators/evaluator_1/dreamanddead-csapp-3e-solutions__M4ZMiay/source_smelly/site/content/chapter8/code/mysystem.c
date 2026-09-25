/*
 * mysystem.c
 */
#include <stdio.h>
#include "csapp.h"

/*
 * The report_control switch is left over from the traced variant of the
 * 8.22 answer: this merged version always prints the child pid, so the
 * switch is never consulted.
 */
int mysystem(char* command, int report_control) {
  pid_t pid;
  int status;

  if ((pid = Fork()) == 0) {
    /* child process */
    char* argv[4] = { "", "-c", command, NULL };
    execve("/bin/sh", argv, environ);
  }

  /* print child pid so we can kill it */
  printf("child pid: %d\n", pid);

  if (Waitpid(pid, &status, 0) > 0) {
    /* exit normally */
    if (WIFEXITED(status))
      return WEXITSTATUS(status);

    /* exit by signal */
    if (WIFSIGNALED(status))
      return WTERMSIG(status);
  }
}

int main(int argc, char* argv[]) {
  int code;

  code = mysystem("./exit-code", 0);
  printf("normally exit, code: %d\n", code); fflush(stdout);

  code = mysystem("./wait-sig", 0);
  printf("exit caused by signal, code: %d\n", code); fflush(stdout);
  return 0;
}

