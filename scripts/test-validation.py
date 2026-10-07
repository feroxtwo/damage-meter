#!/usr/bin/env python3
import importlib.util
import unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('validation',Path(__file__).with_name('validate-combat.py'))
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
class Comparison(unittest.TestCase):
    def report(self):
        return {'parser_rev':'test','targets':[{'details':{'targetId':9,'skills':[{'actorId':1,'code':11,'dmg':1000},{'actorId':2,'code':11,'dmg':9999}], 'healSkills':[{'actorId':1,'code':12,'dmg':300}]}}]}
    def reference(self):
        return [{'kind':'damage','code':'11','is_dot':'0','total':'1000'},{'kind':'heal','code':'12','is_dot':'0','total':'300'}]
    def test_exact_totals_and_actor_isolation(self):
        self.assertTrue(v.compare(self.report(),self.reference(),1,9,0)['passed'])
    def test_per_skill_difference_cannot_be_hidden_by_total(self):
        ref=self.reference();ref[0]['total']='900'
        self.assertFalse(v.compare(self.report(),ref,1,9,.5)['passed'])
    def test_omitted_skill_and_empty_reference_fail(self):
        self.assertFalse(v.compare(self.report(),self.reference()[:1],1,9,.5)['passed'])
        with self.assertRaises(ValueError):v.compare(self.report(),[],1,9,.5)
    def test_duplicate_and_wrong_target_fail(self):
        with self.assertRaises(ValueError):v.compare(self.report(),self.reference()*2,1,9,.5)
        with self.assertRaises(ValueError):v.compare(self.report(),self.reference(),1,10,.5)
unittest.main()
