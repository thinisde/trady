"""Synthetic importer guards. Never opens the runtime database or a broker."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('eur_attribution_import', ROOT/'scripts/import-eur-capability-attribution.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ImportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.evidence = self.root/'evidence.json'
        self.evidence.write_text('{}')
        self.binary = self.root/'synthetic-binary'
        self.binary.write_text('SYNTHETIC_IMPORT_GUARD_ONLY')
        self.registry = self.root/'registry.json'
        self.value = {'passed':True, 'reviewer':'independent-fixture', 'evidence_sha256':module.digest(self.evidence),
                      'reviewed_at':'2026-11-06T11:00:00Z', 'valid_until':'2026-12-06T00:00:00Z',
                      'external_cleanups':[{'owner_id':'owner', 'account_id':'paper_test', 'intent_id':'intent', 'head_revision':15, 'state_sha256':'a'*64}]}
        self.out = self.root/'receipt.json'
        self.image_id='sha256:'+'c'*64
        self.config_image='postgres:18.3'

    def argv(self):
        self.registry.write_text(json.dumps(self.value))
        return ['importer', '--registry',str(self.registry), '--evidence',str(self.evidence), '--binary',str(self.binary),
                '--reviewed-registry-sha256',module.digest(self.registry), '--reviewed-binary-sha256',module.digest(self.binary), '--out',str(self.out)]

    def check_output(self, args, **kwargs):
        if args[:2] == ['docker','inspect']:
            return json.dumps([{'Config':{'Image':self.config_image},'Image':self.image_id,'HostConfig':{},'Mounts':[{'Type':'volume','Name':'trady-eur-postgres18-data','Destination':'/var/lib/postgresql'}]}]).encode()
        if args[:3] == ['docker','image','inspect']:
            return 'sha256:'+'c'*64+'\n'
        if args[:2] == ['docker','port']:
            return '127.0.0.1:32811\n'
        raise AssertionError('unexpected subprocess')

    def run_mock(self, args, **kwargs):
        if 'capability-attribution-check' in args:
            self.assertEqual(kwargs['env'], {})
            return subprocess.CompletedProcess(args,0,json.dumps({'status':'REVIEWED_CAPABILITY_ATTRIBUTION_VALID'}).encode(),b'')
        self.sql = kwargs['input']
        return subprocess.CompletedProcess(args,0,'','')

    def test_exact_reviewed_conflicts_must_be_checked_after_append_only_insert(self):
        with patch('sys.argv', self.argv()), patch.object(module.os,'geteuid',return_value=0), patch.object(module.subprocess,'check_output',side_effect=self.check_output), patch.object(module.subprocess,'run',side_effect=self.run_mock):
            module.main()
        self.assertIn('ON CONFLICT DO NOTHING', self.sql)
        self.assertIn('REVIEWED_CLEANUP_CONFLICT', self.sql)
        self.assertIn('AND evidence_sha256=', self.sql)
        self.assertIn('AND reviewed_at=', self.sql)
        self.assertIn('AND valid_until=', self.sql)
        self.assertIn('pg_advisory_xact_lock(52211463,0)', self.sql)
        self.assertNotIn('UPDATE target.protocol', self.sql)
        receipt=json.loads(self.out.read_text())
        self.assertFalse(receipt['activation_authorized'])
        self.assertFalse(receipt['old_journals_rewritten'])

    def test_pending_or_changed_bytes_never_reach_database(self):
        self.value['passed']=False
        with patch('sys.argv',self.argv()), patch.object(module.os,'geteuid',return_value=0), patch.object(module.subprocess,'run') as run:
            with self.assertRaisesRegex(ValueError,'INDEPENDENT_ATTRIBUTION_REVIEW_REQUIRED'):module.main()
            run.assert_not_called()
        self.value['passed']=True
        args=self.argv()
        self.binary.write_text('CHANGED')
        with patch('sys.argv',args), patch.object(module.os,'geteuid',return_value=0), patch.object(module.subprocess,'run') as run:
            with self.assertRaisesRegex(ValueError,'REVIEWED_BYTES_CHANGED'):module.main()
            run.assert_not_called()
        self.assertFalse(self.out.exists())

    def test_digest_started_container_must_match_exact_postgres_18_3_image(self):
        self.config_image=self.image_id
        with patch('sys.argv',self.argv()), patch.object(module.os,'geteuid',return_value=0), patch.object(module.subprocess,'check_output',side_effect=self.check_output), patch.object(module.subprocess,'run',side_effect=self.run_mock):
            module.main()
        self.assertTrue(self.out.exists())

    def test_other_running_image_never_reaches_import_sql(self):
        self.image_id='sha256:'+'d'*64
        with patch('sys.argv',self.argv()), patch.object(module.os,'geteuid',return_value=0), patch.object(module.subprocess,'check_output',side_effect=self.check_output), patch.object(module.subprocess,'run',side_effect=self.run_mock) as run:
            with self.assertRaisesRegex(ValueError,'PERSISTENT_DEMO_DATABASE_CHANGED'):module.main()
        self.assertEqual(run.call_count,1,'only local proof verifier may run; database SQL remains untouched')
        self.assertFalse(self.out.exists())

    def test_failed_import_never_claims_success_or_writes_receipt(self):
        def fail(args, **kwargs):
            if 'capability-attribution-check' in args:return self.run_mock(args,**kwargs)
            return subprocess.CompletedProcess(args,1,'','fixture SQL conflict')
        with patch('sys.argv',self.argv()), patch.object(module.os,'geteuid',return_value=0), patch.object(module.subprocess,'check_output',side_effect=self.check_output), patch.object(module.subprocess,'run',side_effect=fail):
            with self.assertRaisesRegex(ValueError,'REVIEWED_CLEANUP_IMPORT_STOPPED'):module.main()
        self.assertFalse(self.out.exists())


if __name__ == '__main__':
    unittest.main()
