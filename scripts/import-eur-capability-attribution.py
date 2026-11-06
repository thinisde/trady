#!/usr/bin/env python3
"""Import exact reviewed cleanup exceptions; never submits orders or activates a release.

The reviewed role labels native history and freezes old externally cleaned
journals. It cannot supply partial/paper evidence or an owner approval receipt.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

NAME = 'trady-live-demo-db-20261031'

def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as data:
        for block in iter(lambda: data.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()

def quote(value):
    return "'" + str(value).replace("'", "''") + "'"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--registry', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--reviewed-registry-sha256', required=True)
    parser.add_argument('--reviewed-binary-sha256', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() != 0 or args.out.exists():
        raise ValueError('TRUSTED_ROOT_AND_NEW_RECEIPT_REQUIRED')
    if digest(args.registry) != args.reviewed_registry_sha256 or digest(args.binary) != args.reviewed_binary_sha256:
        raise ValueError('REVIEWED_BYTES_CHANGED')
    value = json.loads(args.registry.read_text())
    if not value.get('passed') or not value.get('reviewer') or value['evidence_sha256'] != digest(args.evidence):
        raise ValueError('INDEPENDENT_ATTRIBUTION_REVIEW_REQUIRED')
    proof = subprocess.run([str(args.binary.resolve()), 'eur', 'capability-attribution-check', '--registry', str(args.registry.resolve()), '--evidence', str(args.evidence.resolve())], env={}, capture_output=True, timeout=30)
    if proof.returncode or json.loads(proof.stdout)['status'] != 'REVIEWED_CAPABILITY_ATTRIBUTION_VALID':
        raise ValueError('EXACT_NATIVE_PROOF_INVALID')
    inspection = json.loads(subprocess.check_output(['docker', 'inspect', NAME]))[0]
    image = subprocess.check_output(['docker','image','inspect','postgres:18.3','--format','{{.Id}}'],text=True).strip()
    if (not re.fullmatch(r'sha256:[a-f0-9]{64}',image) or inspection['Image'] != image
            or inspection['Config']['Image'] not in ['postgres:18.3',image]
            or inspection['HostConfig'].get('Tmpfs')
            or not any(m['Type'] == 'volume' and m.get('Name') == 'trady-eur-postgres18-data'
                       and m['Destination'] == '/var/lib/postgresql' for m in inspection['Mounts'])):
        raise ValueError('PERSISTENT_DEMO_DATABASE_CHANGED')
    if subprocess.check_output(['docker','port',NAME,'5432/tcp'],text=True).strip() != '127.0.0.1:32811':
        raise ValueError('DEMO_DATABASE_ENDPOINT_CHANGED')
    statements = ['BEGIN;', 'SET LOCAL ROLE trady_owner;', "SET LOCAL lock_timeout='10s';", 'SELECT pg_advisory_xact_lock(52211463,0);']
    for entry in value['external_cleanups']:
        if not all(re.fullmatch(r'[A-Za-z0-9_.:-]{1,120}', entry[k]) for k in ['owner_id','account_id','intent_id']) or not re.fullmatch(r'[a-f0-9]{64}',entry['state_sha256']):
            raise ValueError('INVALID_ORIGINAL_SCOPE')
        owner, account, intent = [quote(entry[k]) for k in ['owner_id','account_id','intent_id']]
        revision = int(entry['head_revision'])
        if revision < 1:
            raise ValueError('INVALID_ORIGINAL_HEAD')
        state = quote(entry['state_sha256'])
        statements.append(f"""DO $$ BEGIN
 PERFORM 1 FROM target.protocol_heads WHERE owner_id={owner} AND account_id={account} AND intent_id={intent}
 AND revision={revision} AND state_sha256={state} AND context->>'runtime_mode'='DEMO' AND context->>'broker_account_id'='52211463' FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'REVIEWED_HEAD_CHANGED'; END IF;
 END $$;""")
        statements.append(f"INSERT INTO target.reviewed_external_cleanups VALUES({owner},{account},{intent},{revision},{state},{quote(value['evidence_sha256'])},{quote(args.reviewed_registry_sha256)},{quote(value['reviewed_at'])}::timestamptz,{quote(value['valid_until'])}::timestamptz) ON CONFLICT DO NOTHING;")
        statements.append(f"""DO $$ BEGIN
 PERFORM 1 FROM target.reviewed_external_cleanups WHERE owner_id={owner} AND account_id={account} AND intent_id={intent}
 AND head_revision={revision} AND state_sha256={state} AND review_sha256={quote(args.reviewed_registry_sha256)}
 AND evidence_sha256={quote(value['evidence_sha256'])} AND reviewed_at={quote(value['reviewed_at'])}::timestamptz AND valid_until={quote(value['valid_until'])}::timestamptz;
 IF NOT FOUND THEN RAISE EXCEPTION 'REVIEWED_CLEANUP_CONFLICT'; END IF;
 END $$;""")
    statements.append('COMMIT;')
    result = subprocess.run(['docker','exec','-i','--user','postgres',NAME,'psql','-X','-q','-v','ON_ERROR_STOP=1','-U','postgres','-d','trady'],input='\n'.join(statements),text=True,capture_output=True,timeout=30)
    if result.returncode:
        raise ValueError('REVIEWED_CLEANUP_IMPORT_STOPPED_NO_APPROVAL_CREATED')
    receipt = {'kind':'REVIEWED_CAPABILITY_CLEANUP_IMPORT_NOT_RELEASE_APPROVAL','recorded_at':datetime.now(timezone.utc).isoformat(),'registry_sha256':args.reviewed_registry_sha256,'binary_sha256':args.reviewed_binary_sha256,'evidence_sha256':value['evidence_sha256'],'exact_cleanup_scopes':value['external_cleanups'],'old_journals_rewritten':False,'broker_mutations':0,'activation_authorized':False,'live_enabled':False}
    with os.fdopen(os.open(args.out, os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w') as out:
        json.dump(receipt,out,indent=2);out.flush();os.fsync(out.fileno())
    print(json.dumps({'status':'EXACT_REVIEWED_CLEANUPS_IMPORTED','scope_count':len(value['external_cleanups']),'activation_authorized':False}))

if __name__ == '__main__':
    main()
