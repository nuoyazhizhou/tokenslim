import { TSRehydrator } from './rehydrator';

const rehydrator = new TSRehydrator();

function init() {
    const observer = new MutationObserver((mutations) => {
        for (const mutation of mutations) {
            for (const node of mutation.addedNodes) {
                if (node instanceof HTMLElement) {
                    processNode(node);
                }
            }
        }
    });

    observer.observe(document.body, { childList: true, subtree: true });
    processNode(document.body);
}

function processNode(root: HTMLElement) {
    const codeBlocks = root.querySelectorAll('pre, code');
    for (const block of codeBlocks) {
        if (block.hasAttribute('data-tokenslim-processed')) continue;
        
        const text = block.textContent || "";
        if (text.includes('"tokens":') && text.includes('"dictionary":')) {
            try {
                // Heuristic check to see if it's likely a TokenSlim JSON
                const json = JSON.parse(text);
                if (json.tokens && json.dictionary) {
                    injectRestoreButton(block as HTMLElement, json);
                }
            } catch (e) {
                // Not valid JSON or too partial
            }
        }
    }
}

function injectRestoreButton(target: HTMLElement, payload: any) {
    const container = target.parentElement;
    if (!container) return;

    target.setAttribute('data-tokenslim-processed', 'true');

    const button = document.createElement('button');
    button.innerText = 'TokenSlim: Restore Logs';
    button.className = 'tokenslim-restore-btn';
    
    button.onclick = () => {
        try {
            const restoredText = rehydrator.rehydrate(payload);
            target.textContent = restoredText;
            button.remove();
            
            // Add a small badge
            const badge = document.createElement('span');
            badge.innerText = '✓ Restored by TokenSlim';
            badge.className = 'tokenslim-badge';
            target.prepend(badge);
        } catch (e) {
            console.error('TokenSlim Rehydration failed', e);
            button.innerText = 'Restoration Failed';
        }
    };

    if (container.style.position === '') {
        container.style.position = 'relative';
    }
    container.appendChild(button);
}

init();
