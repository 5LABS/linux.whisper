/* type_de.c – German QWERTZ key injector via ydotool
 * Reads UTF-8 text from stdin, maps each character to the correct
 * physical evdev keycode for a German layout, and calls ydotool key.
 * Drop-in replacement for type_de.py. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>
#include <stdint.h>

#define SHIFT  42
#define ALTGR  100
#define MAX_ARGS 8192
#define CHUNK    400

struct Entry { uint32_t cp; int kc; int mod; };

static const struct Entry KEYMAP[] = {
    /* lowercase */
    {'a',30,0},{'b',48,0},{'c',46,0},{'d',32,0},{'e',18,0},{'f',33,0},
    {'g',34,0},{'h',35,0},{'i',23,0},{'j',36,0},{'k',37,0},{'l',38,0},
    {'m',50,0},{'n',49,0},{'o',24,0},{'p',25,0},{'q',16,0},{'r',19,0},
    {'s',31,0},{'t',20,0},{'u',22,0},{'v',47,0},{'w',17,0},{'x',45,0},
    {'y',44,0},{'z',21,0},
    /* umlauts lowercase */
    {0x00e4,40,0},{0x00f6,39,0},{0x00fc,26,0},{0x00df,12,0},
    /* uppercase */
    {'A',30,SHIFT},{'B',48,SHIFT},{'C',46,SHIFT},{'D',32,SHIFT},
    {'E',18,SHIFT},{'F',33,SHIFT},{'G',34,SHIFT},{'H',35,SHIFT},
    {'I',23,SHIFT},{'J',36,SHIFT},{'K',37,SHIFT},{'L',38,SHIFT},
    {'M',50,SHIFT},{'N',49,SHIFT},{'O',24,SHIFT},{'P',25,SHIFT},
    {'Q',16,SHIFT},{'R',19,SHIFT},{'S',31,SHIFT},{'T',20,SHIFT},
    {'U',22,SHIFT},{'V',47,SHIFT},{'W',17,SHIFT},{'X',45,SHIFT},
    {'Y',44,SHIFT},{'Z',21,SHIFT},
    /* umlauts uppercase */
    {0x00c4,40,SHIFT},{0x00d6,39,SHIFT},{0x00dc,26,SHIFT},
    /* digits */
    {'1',2,0},{'2',3,0},{'3',4,0},{'4',5,0},{'5',6,0},
    {'6',7,0},{'7',8,0},{'8',9,0},{'9',10,0},{'0',11,0},
    /* whitespace */
    {' ',57,0},{'\t',15,0},
    /* punctuation no modifier */
    {'.',52,0},{',',51,0},{'-',53,0},{'+',27,0},{'#',43,0},{'<',86,0},
    /* punctuation shift */
    {'!',2,SHIFT},{'"',3,SHIFT},{0x00a7,4,SHIFT},{'$',5,SHIFT},
    {'%',6,SHIFT},{'&',7,SHIFT},{'/',8,SHIFT},{'(',9,SHIFT},
    {')',10,SHIFT},{'=',11,SHIFT},{'?',12,SHIFT},{'*',27,SHIFT},
    {'\'',43,SHIFT},{';',51,SHIFT},{':',52,SHIFT},{'_',53,SHIFT},
    {'>',86,SHIFT},
    /* altgr */
    {'@',16,ALTGR},{0x20ac,18,ALTGR},{'{',8,ALTGR},{'[',9,ALTGR},
    {']',10,ALTGR},{'}',11,ALTGR},{'\\',12,ALTGR},{'~',27,ALTGR},
    {'|',86,ALTGR},
    {0,0,0}
};

/* Normalize typographic characters to ASCII equivalents.
 * Returns 0 if the codepoint should expand to multiple chars (handled inline). */
static uint32_t normalize(uint32_t cp) {
    switch (cp) {
        case 0x201e: case 0x201c: case 0x201d:
        case 0x00bb: case 0x00ab: return '"';
        case 0x201a: case 0x2018: case 0x2019: return '\'';
        case 0x2013: case 0x2014: return '-';
        case 0x00a0: return ' ';
        default: return cp;
    }
}

static const struct Entry *lookup(uint32_t cp) {
    for (int i = 0; KEYMAP[i].cp; i++)
        if (KEYMAP[i].cp == cp) return &KEYMAP[i];
    return NULL;
}

/* Read one UTF-8 codepoint from stdin. Returns 0 on EOF, UINT32_MAX on error. */
static uint32_t read_cp(void) {
    int c = getchar();
    if (c == EOF) return 0;
    uint32_t cp; int extra;
    if      ((c & 0x80) == 0x00) { cp = c;        extra = 0; }
    else if ((c & 0xe0) == 0xc0) { cp = c & 0x1f; extra = 1; }
    else if ((c & 0xf0) == 0xe0) { cp = c & 0x0f; extra = 2; }
    else if ((c & 0xf8) == 0xf0) { cp = c & 0x07; extra = 3; }
    else return UINT32_MAX;
    for (int i = 0; i < extra; i++) {
        c = getchar();
        if (c == EOF) return UINT32_MAX;
        cp = (cp << 6) | (c & 0x3f);
    }
    return cp;
}

/* args buffer – each entry is a small "kc:val" string */
static char args[MAX_ARGS][12];
static int  nargs = 0;

static void push(const struct Entry *e) {
    if (!e || nargs + 4 >= MAX_ARGS) return;
    if (e->mod) snprintf(args[nargs++], 12, "%d:1", e->mod);
    snprintf(args[nargs++], 12, "%d:1", e->kc);
    snprintf(args[nargs++], 12, "%d:0", e->kc);
    if (e->mod) snprintf(args[nargs++], 12, "%d:0", e->mod);
}

static void flush_chunk(int start, int end) {
    int cnt = end - start;
    char **argv = malloc((6 + cnt) * sizeof(char *));
    if (!argv) return;
    argv[0] = "ydotool";
    argv[1] = "key";
    argv[2] = "--key-delay";
    argv[3] = "6";
    for (int j = 0; j < cnt; j++) argv[4 + j] = args[start + j];
    argv[4 + cnt] = NULL;
    pid_t pid = fork();
    if (pid == 0) { execvp("ydotool", argv); _exit(1); }
    if (pid > 0)  { int s; waitpid(pid, &s, 0); }
    free(argv);
}

int main(void) {
    /* set YDOTOOL_SOCKET if not already in env */
    if (!getenv("YDOTOOL_SOCKET")) {
        char buf[64];
        snprintf(buf, sizeof(buf), "/run/user/%d/.ydotool_socket", getuid());
        setenv("YDOTOOL_SOCKET", buf, 0);
    }

    uint32_t cp;
    while ((cp = read_cp()) && cp != UINT32_MAX) {
        cp = normalize(cp);
        if (cp == 0x2026) { /* ellipsis → three dots */
            const struct Entry *dot = lookup('.');
            push(dot); push(dot); push(dot);
            continue;
        }
        push(lookup(cp));
    }

    for (int i = 0; i < nargs; i += CHUNK)
        flush_chunk(i, i + CHUNK < nargs ? i + CHUNK : nargs);

    return 0;
}
