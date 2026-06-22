export interface TokenSlimPayload {
    tokens: any[];
    dictionary: {
        paths: Record<string, string>;
        packages: Record<string, string>;
        macros: Record<string, string>;
        files: Record<string, string>;
        directories: Record<string, string>;
        flags: Record<string, string>;
        custom: Record<string, Record<string, string>>;
    };
}

export class TSRehydrator {
    public rehydrate(payload: TokenSlimPayload): string {
        let text = "";
        for (const token of payload.tokens) {
            if (typeof token === 'string') {
                text += token;
            } else if (token.Text !== undefined) {
                text += token.Text;
            } else if (token.DictRef !== undefined) {
                text += this.resolveRecursive(token.DictRef, payload);
            } else if (token.Repeat !== undefined) {
                const repeated = this.rehydrate({ tokens: [token.Repeat.token], dictionary: payload.dictionary });
                text += repeated.repeat(token.Repeat.count);
            }
        }

        return this.restoreMarkers(text);
    }

    private resolveRecursive(token: string, payload: TokenSlimPayload, depth = 0): string {
        if (depth > 10) return token; // Prevent infinite loops

        const dict = payload.dictionary;
        let original: string | undefined;

        if (token.startsWith("$PK")) original = dict.packages[token];
        else if (token.startsWith("$FL")) original = dict.flags[token];
        else if (token.startsWith("$P")) original = dict.paths[token];
        else if (token.startsWith("$M")) original = dict.macros[token];
        else if (token.startsWith("$F")) original = dict.files[token];
        else if (token.startsWith("$D")) original = dict.directories[token];
        else {
            // Check custom
            for (const typeMap of Object.values(dict.custom)) {
                if (typeMap[token]) {
                    original = typeMap[token];
                    break;
                }
            }
        }

        if (!original) return token;

        // If the resolved text contains other tokens, resolve them too
        if (original.includes("$")) {
            const tokenRegex = /\$(?:P|D|M|F|PK|FL)\d+/g;
            return original.replace(tokenRegex, (match) => this.resolveRecursive(match, payload, depth + 1));
        }

        return original;
    }

    private restoreMarkers(text: string): string {
        return text
            .replace(/\$PL /g, "[Pipeline] ")
            .replace(/\$PL/g, "[Pipeline]")
            .replace(/\$GCC /g, "gcc: ")
            .replace(/\$XC\|PROBE\|x(\d+)/g, (_, count) => `[Xcode Probe x${count}]`)
            .replace(/\$XC\|AGG\|([^|]+)\|x(\d+)/g, (_, type, count) => `[${type} x${count}]`)
            .replace(/\[RES_WARN_AGG: (.*?): \[(.*?), \.\.\., (.*?)\] \(total (\d+)\)\]/g, 
                (_, pkg, first, last, total) => `[Android Resource Warning Aggregation: ${pkg} (${total} items: ${first}...${last})]`);
    }
}
